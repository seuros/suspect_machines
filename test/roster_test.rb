# frozen_string_literal: true
require 'minitest/autorun'
require 'suspect_machines'

class RosterTest < Minitest::Test
  SECOND = 1_000_000_000
  MILLI = 1_000_000

  def normal(min_samples: 3, floor: 100 * MILLI, **rest)
    SuspectMachines::Roster.new(distribution: :normal, min_samples: min_samples, min_std_dev_ns: floor, **rest)
  end

  def exponential(min_samples: 1, **rest)
    SuspectMachines::Roster.new(distribution: :exponential, min_samples: min_samples, **rest)
  end

  def test_warm_up_then_phi_under_the_normal_assumption
    roster = normal
    assert_equal :untracked, roster.suspicion(7, 0)
    assert_equal :first, roster.heartbeat(7, 1, 0)
    assert_equal :warming_up, roster.suspicion(7, 60 * SECOND)
    assert_equal :recorded, roster.heartbeat(7, 1, 1 * SECOND)
    assert_equal :recorded, roster.heartbeat(7, 1, 2 * SECOND)
    assert_equal :warming_up, roster.suspicion(7, 3 * SECOND)
    assert_equal :recorded, roster.heartbeat(7, 1, 3 * SECOND)
    # Regular 1 s beats, floor 100 ms: on time is the median, 1.3 s is z = 3.
    assert_in_delta Math.log10(2), roster.suspicion(7, 4 * SECOND), 1e-9
    assert_in_delta 2.869699035929369, roster.suspicion(7, 4 * SECOND + 300 * MILLI), 1e-9
    assert_operator roster.suspicion(7, 10 * SECOND), :>, 100
    assert_in_delta 0.0, roster.suspicion(7, 3 * SECOND), 1e-9
    assert_equal :time_regression, roster.suspicion(7, 3 * SECOND - 1)
  end

  def test_exponential_assumption_and_acceptable_pause
    roster = exponential
    lenient = exponential(acceptable_pause_ns: 5)
    [0, 10, 20].each do |now|
      roster.heartbeat(1, 0, now)
      lenient.heartbeat(1, 0, now)
    end
    assert_in_delta 3 / Math.log(10), roster.suspicion(1, 50), 1e-12
    assert_in_delta 3 / Math.log(10), lenient.suspicion(1, 55), 1e-12
    assert_equal 0.0, lenient.suspicion(1, 24)
  end

  def test_statistics_and_window_sliding
    roster = exponential
    assert_nil roster.statistics(1)
    roster.heartbeat(1, 0, 0)
    assert_nil roster.statistics(1)
    assert_equal 0, roster.last_heartbeat_ns(1)
    now = 0
    [30, 40, 100].each { |gap| roster.heartbeat(1, 0, now += gap) }
    stats = roster.statistics(1)
    assert_instance_of SuspectMachines::Statistics, stats
    assert_equal 3, stats.samples
    assert_in_delta 170 / 3.0, stats.mean_ns, 1e-9
    assert_in_delta Math.sqrt((900 + 1600 + 10_000) / 3.0 - (170 / 3.0)**2), stats.std_dev_ns, 1e-9
    assert_equal [30, 100], [stats.min_ns, stats.max_ns]
    assert_equal 170, roster.last_heartbeat_ns(1)
    assert_nil roster.last_heartbeat_ns(2)

    (SuspectMachines::Roster::WINDOW + 5).times { roster.heartbeat(1, 0, now += 7) }
    stats = roster.statistics(1)
    assert_equal SuspectMachines::Roster::WINDOW, stats.samples
    assert_equal [7, 7, 7.0, 0.0], [stats.min_ns, stats.max_ns, stats.mean_ns, stats.std_dev_ns]
  end

  def test_refusals_are_symbols_and_incarnations_fence_old_senders
    roster = exponential
    assert_equal :first, roster.heartbeat(1, 5, 100)
    assert_equal :duplicate, roster.heartbeat(1, 5, 100)
    assert_equal :time_regression, roster.heartbeat(1, 5, 99)
    assert_equal :stale, roster.heartbeat(1, 4, 200)
    assert_equal :recorded, roster.heartbeat(1, 5, 200)
    assert_equal 5, roster.incarnation(1)
    assert_equal :time_regression, roster.heartbeat(1, 6, 199)
    assert_equal :restarted, roster.heartbeat(1, 6, 200)
    assert_equal 6, roster.incarnation(1)
    assert_nil roster.statistics(1)
    assert_equal :warming_up, roster.suspicion(1, 10_000)
    assert_nil roster.incarnation(2)
  end

  def test_capacity_forget_and_size
    roster = exponential
    assert_equal 64, SuspectMachines::Roster::CAPACITY
    assert_equal 128, SuspectMachines::Roster::WINDOW
    assert_equal [64, 128, 0], [roster.capacity, roster.window, roster.size]
    64.times { |peer| assert_equal :first, roster.heartbeat(peer, 0, 0) }
    assert_equal 64, roster.size
    assert_equal :full, roster.heartbeat(64, 0, 0)
    assert roster.forget(3)
    refute roster.forget(3)
    assert_equal 63, roster.size
    assert_equal :first, roster.heartbeat(64, 0, 0)
    assert_equal :full, roster.heartbeat(3, 0, 0)
  end

  def test_suspects_pages_through_a_bounded_scan
    roster = exponential(min_samples: 2)
    5.times do |peer|
      (0..(peer + 2)).each { |beat| roster.heartbeat(peer, peer, beat * 10) }
    end
    roster.heartbeat(5, 0, 0)
    roster.heartbeat(5, 0, 10) # one interval: still warming up
    scan = roster.suspects(120, 3.0)
    assert_instance_of SuspectMachines::Scan, scan
    assert_nil scan.next_cursor
    assert_equal [0, 1, 2, 3], scan.suspects.map(&:peer)
    assert_equal [0, 1, 2, 3], scan.suspects.map(&:incarnation)
    assert_in_delta 10 / Math.log(10), scan.suspects.first.phi, 1e-12
    assert_instance_of SuspectMachines::Suspect, scan.suspects.first

    page = roster.suspects(120, 3, limit: 3)
    assert_equal [[0, 1, 2], 3], [page.suspects.map(&:peer), page.next_cursor]
    rest = roster.suspects(120, 3, cursor: page.next_cursor, limit: 3)
    assert_equal [[3], nil], [rest.suspects.map(&:peer), rest.next_cursor]
    assert_equal [[], 0], roster.suspects(120, 3, limit: 0).then { [it.suspects, it.next_cursor] }
    assert_equal [[], nil], roster.suspects(120, 3, cursor: 10_000).then { [it.suspects, it.next_cursor] }
    assert_empty roster.suspects(120, 1e6).suspects
    assert roster.suspects(120, 3).frozen?
  end

  def test_strict_inputs_raise_before_mutation
    roster = exponential
    bad = [1.0, Float::NAN, nil, '1', Object.new]
    bad.each do |value|
      assert_raises(TypeError) { roster.heartbeat(value, 0, 0) }
      assert_raises(TypeError) { roster.heartbeat(1, value, 0) }
      assert_raises(TypeError) { roster.heartbeat(1, 0, value) }
      assert_raises(TypeError) { roster.suspicion(value, 0) }
      assert_raises(TypeError) { roster.suspicion(1, value) }
      assert_raises(TypeError) { roster.statistics(value) }
      assert_raises(TypeError) { roster.incarnation(value) }
      assert_raises(TypeError) { roster.last_heartbeat_ns(value) }
      assert_raises(TypeError) { roster.forget(value) }
      assert_raises(TypeError) { roster.suspects(value, 1.0) }
      assert_raises(TypeError) { roster.suspects(0, 1.0, cursor: value) }
      assert_raises(TypeError) { roster.suspects(0, 1.0, limit: value) }
    end
    [nil, '8', Object.new, :high].each { |value| assert_raises(TypeError) { roster.suspects(0, value) } }
    [0, -1, 0.0, Float::NAN, Float::INFINITY].each do |value|
      assert_raises(RangeError) { roster.suspects(0, value) }
    end
    [-1, 2**64].each do |n|
      assert_raises(RangeError) { roster.heartbeat(n, 0, 0) }
      assert_raises(RangeError) { roster.heartbeat(1, n, 0) }
      assert_raises(RangeError) { roster.heartbeat(1, 0, n) }
      assert_raises(RangeError) { roster.suspects(n, 1.0) }
    end
    assert_raises(RangeError) { roster.suspects(0, 1.0, limit: 65) }
    assert_equal 0, roster.size
  end

  def test_configuration_is_explicit_and_validated
    assert_raises(ArgumentError) { SuspectMachines::Roster.new }
    assert_raises(ArgumentError) { SuspectMachines::Roster.new(distribution: :normal, min_samples: 3) }
    assert_raises(ArgumentError) { exponential(min_std_dev_ns: 5) }
    assert_raises(ArgumentError) { SuspectMachines::Roster.new(distribution: :weibull, min_samples: 3) }
    assert_raises(TypeError) { SuspectMachines::Roster.new(distribution: 'normal', min_samples: 3) }
    assert_raises(TypeError) { normal(min_samples: 3.0) }
    assert_raises(TypeError) { normal(floor: 1.5) }
    assert_raises(TypeError) { exponential(acceptable_pause_ns: nil) }
    assert_raises(RangeError) { normal(min_samples: 0) }
    assert_raises(RangeError) { normal(min_samples: 129) }
    assert_raises(RangeError) { normal(floor: 0) }
    assert_raises(RangeError) { normal(floor: -1) }
    assert_raises(RangeError) { exponential(acceptable_pause_ns: 2**64) }
    assert_instance_of SuspectMachines::Roster, normal(min_samples: 128, floor: 2**64 - 1)
    assert_raises(NoMethodError) { SuspectMachines::Roster._new(:normal, 1, 1, 0) }
    assert_raises(NoMethodError) { exponential._suspects(0, 1.0, 0, 1) }
    assert_raises(TypeError) { SuspectMachines::Roster.allocate }
  end

  def test_freeze_guards_writes_and_keeps_reads
    roster = exponential
    roster.heartbeat(1, 0, 0)
    roster.heartbeat(1, 0, 10)
    roster.freeze
    assert_raises(FrozenError) { roster.heartbeat(1, 0, 20) }
    assert_raises(FrozenError) { roster.forget(1) }
    assert_in_delta 1 / Math.log(10), roster.suspicion(1, 20), 1e-12
    assert_equal [1], roster.suspects(20, 0.1).suspects.map(&:peer)
    assert_equal 1, roster.statistics(1).samples
    assert_equal [1, 0, 10], [roster.size, roster.incarnation(1), roster.last_heartbeat_ns(1)]
  end

  def test_u64_limits_and_gc_stress
    previous = GC.stress
    GC.stress = true
    max = 2**64 - 1
    roster = normal(min_samples: 1, floor: 1)
    assert_equal :first, roster.heartbeat(max, max, 0)
    assert_equal :recorded, roster.heartbeat(max, max, 1)
    phi = roster.suspicion(max, max)
    assert phi.finite?
    assert_operator phi, :>, 1e37
    suspect = roster.suspects(max, 1e30).suspects.fetch(0)
    assert_equal [max, max], [suspect.peer, suspect.incarnation]
    assert_equal :recorded, roster.heartbeat(max, max, max)
    assert_equal max - 1, roster.statistics(max).max_ns
    GC.compact
    assert_equal :duplicate, roster.heartbeat(max, max, max)
  ensure
    GC.stress = previous
  end
end
