# frozen_string_literal: true
require 'minitest/autorun'
require 'suspect_machines'

class RosterTest < Minitest::Test
  SECOND = 1_000_000_000
  MILLI = 1_000_000

  Recorded = SuspectMachines::Recorded
  WarmingUp = SuspectMachines::WarmingUp
  Measured = SuspectMachines::Measured
  Normal = SuspectMachines::Normal
  Config = SuspectMachines::Config

  def test_values_mirror_the_core
    names = %i[Phi Threshold Normal Config Recorded WarmingUp Measured Statistics Suspect Scan]
    names.each { assert_operator SuspectMachines.const_get(_1), :<, Data }
    assert_equal %i[value], SuspectMachines::Phi.members
    assert_equal %i[value], SuspectMachines::Threshold.members
    assert_equal %i[min_std_dev], Normal.members
    assert_equal %i[distribution min_samples acceptable_pause], Config.members
    assert_equal %i[interval samples], Recorded.members
    assert_equal %i[samples required silence], WarmingUp.members
    assert_equal %i[phi], Measured.members
    assert_equal %i[samples mean_ns std_dev_ns min max], SuspectMachines::Statistics.members
    assert_equal %i[peer incarnation phi], SuspectMachines::Suspect.members
    assert_equal %i[suspects next], SuspectMachines::Scan.members
  end

  def normal_config(min_samples: 3, floor: 100 * MILLI) = Config.new(Normal.new(floor), min_samples)
  def normal(...) = SuspectMachines::Roster.new(normal_config(...))

  def exponential(min_samples: 1, acceptable_pause: nil)
    config = Config.new(:exponential, min_samples)
    config = config.with_acceptable_pause(acceptable_pause) if acceptable_pause
    SuspectMachines::Roster.new(config)
  end

  def phi(roster, peer, now) = roster.suspicion(peer, now).phi.value
  def level(value) = SuspectMachines::Threshold.new(value)

  def test_warm_up_then_phi_under_the_normal_assumption
    roster = normal
    assert_equal :untracked, roster.suspicion(7, 0)
    assert_equal :first, roster.heartbeat(7, 1, 0)
    assert_equal WarmingUp.new(samples: 0, required: 3, silence: 60 * SECOND), roster.suspicion(7, 60 * SECOND)
    assert_equal Recorded.new(interval: SECOND, samples: 1), roster.heartbeat(7, 1, 1 * SECOND)
    assert_equal Recorded.new(SECOND, 2), roster.heartbeat(7, 1, 2 * SECOND)
    assert_equal WarmingUp.new(2, 3, SECOND), roster.suspicion(7, 3 * SECOND)
    assert_predicate roster.suspicion(7, 3 * SECOND), :frozen?
    assert_equal Recorded.new(SECOND, 3), roster.heartbeat(7, 1, 3 * SECOND)
    # Regular 1 s beats, floor 100 ms: on time is the median, 1.3 s is z = 3.
    assert_in_delta Math.log10(2), phi(roster, 7, 4 * SECOND), 1e-9
    assert_in_delta 2.869699035929369, phi(roster, 7, 4 * SECOND + 300 * MILLI), 1e-9
    assert_operator phi(roster, 7, 10 * SECOND), :>, 100
    assert_in_delta 0.0, phi(roster, 7, 3 * SECOND), 1e-9
    assert_equal :time_regression, roster.suspicion(7, 3 * SECOND - 1)
  end

  def test_exponential_assumption_and_acceptable_pause
    roster = exponential
    lenient = exponential(acceptable_pause: 5)
    [0, 10, 20].each do |now|
      roster.heartbeat(1, 0, now)
      lenient.heartbeat(1, 0, now)
    end
    assert_in_delta 3 / Math.log(10), phi(roster, 1, 50), 1e-12
    assert_in_delta 3 / Math.log(10), phi(lenient, 1, 55), 1e-12
    assert_equal Measured.new(SuspectMachines::Phi.new(0.0)), lenient.suspicion(1, 24)
    assert_equal [5, 0], [lenient.config.acceptable_pause, roster.config.acceptable_pause]
  end

  def test_statistics_and_window_sliding
    roster = exponential
    assert_nil roster.statistics(1)
    roster.heartbeat(1, 0, 0)
    assert_nil roster.statistics(1)
    assert_equal 0, roster.last_heartbeat(1)
    now = 0
    [30, 40, 100].each { |gap| roster.heartbeat(1, 0, now += gap) }
    stats = roster.statistics(1)
    assert_instance_of SuspectMachines::Statistics, stats
    assert_equal 3, stats.samples
    assert_in_delta 170 / 3.0, stats.mean_ns, 1e-9
    assert_in_delta Math.sqrt((900 + 1600 + 10_000) / 3.0 - (170 / 3.0)**2), stats.std_dev_ns, 1e-9
    assert_equal [30, 100], [stats.min, stats.max]
    assert_equal 170, roster.last_heartbeat(1)
    assert_nil roster.last_heartbeat(2)

    (SuspectMachines::Roster::WINDOW + 5).times { roster.heartbeat(1, 0, now += 7) }
    stats = roster.statistics(1)
    assert_equal SuspectMachines::Roster::WINDOW, stats.samples
    assert_equal [7, 7, 7.0, 0.0], [stats.min, stats.max, stats.mean_ns, stats.std_dev_ns]
  end

  def test_refusals_are_symbols_and_incarnations_fence_old_senders
    roster = exponential
    assert_equal :first, roster.heartbeat(1, 5, 100)
    assert_equal :same_instant, roster.heartbeat(1, 5, 100)
    assert_equal :time_regression, roster.heartbeat(1, 5, 99)
    assert_equal :stale, roster.heartbeat(1, 4, 200)
    assert_equal Recorded.new(100, 1), roster.heartbeat(1, 5, 200)
    assert_equal 5, roster.incarnation(1)
    assert_equal :time_regression, roster.heartbeat(1, 6, 199)
    assert_equal :restarted, roster.heartbeat(1, 6, 200)
    assert_equal 6, roster.incarnation(1)
    assert_nil roster.statistics(1)
    assert_equal WarmingUp.new(0, 1, 9_800), roster.suspicion(1, 10_000)
    assert_nil roster.incarnation(2)
  end

  def test_capacity_forget_and_size
    roster = exponential
    assert_equal 64, SuspectMachines::Roster::CAPACITY
    assert_equal 128, SuspectMachines::Roster::WINDOW
    assert_equal [64, 128, 0, true], [roster.capacity, roster.window, roster.size, roster.empty?]
    assert_equal Config.new(:exponential, 1), roster.config
    assert_equal normal_config, normal.config
    64.times { |peer| assert_equal :first, roster.heartbeat(peer, 0, 0) }
    assert_equal [64, false], [roster.size, roster.empty?]
    assert_equal :full, roster.heartbeat(64, 0, 0)
    assert_equal :forgotten, roster.forget(3)
    assert_equal :not_tracked, roster.forget(3)
    assert_equal :not_tracked, roster.forget(2**64 - 1)
    assert_equal 63, roster.size
    assert_equal :first, roster.heartbeat(64, 0, 0)
    assert_equal :full, roster.heartbeat(3, 0, 0)
  end

  def test_scan_pages_through_the_roster
    roster = exponential(min_samples: 2)
    5.times do |peer|
      (0..(peer + 2)).each { |beat| roster.heartbeat(peer, peer, beat * 10) }
    end
    roster.heartbeat(5, 0, 0)
    roster.heartbeat(5, 0, 10) # one interval: still warming up
    scan = roster.scan(120, level(3.0), 0, 64)
    assert_instance_of SuspectMachines::Scan, scan
    assert_nil scan.next
    assert_equal [0, 1, 2, 3], scan.suspects.map(&:peer)
    assert_equal [0, 1, 2, 3], scan.suspects.map(&:incarnation)
    assert_in_delta 10 / Math.log(10), scan.suspects.first.phi.value, 1e-12
    assert_instance_of SuspectMachines::Suspect, scan.suspects.first

    page = roster.scan(120, level(3), 0, 3)
    assert_equal [[0, 1, 2], 3], [page.suspects.map(&:peer), page.next]
    rest = roster.scan(120, level(3), page.next, 3)
    assert_equal [[3], nil], [rest.suspects.map(&:peer), rest.next]
    assert_equal [[], 0], roster.scan(120, level(3), 0, 0).then { [it.suspects, it.next] }
    assert_equal [[], nil], roster.scan(120, level(3), 10_000, 64).then { [it.suspects, it.next] }
    assert_empty roster.scan(120, level(1e6), 0, 64).suspects
    assert roster.scan(120, level(3), 0, 64).frozen?
    assert roster.scan(120, level(3), 0, 64).suspects.frozen?
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
      assert_raises(TypeError) { roster.last_heartbeat(value) }
      assert_raises(TypeError) { roster.forget(value) }
      assert_raises(TypeError) { roster.scan(value, level(1.0), 0, 64) }
      assert_raises(TypeError) { roster.scan(0, level(1.0), value, 64) }
      assert_raises(TypeError) { roster.scan(0, level(1.0), 0, value) }
    end
    [nil, '8', Object.new, :high, 1.0, 3, SuspectMachines::Phi.new(1)].each do |value|
      assert_raises(TypeError) { roster.scan(0, value, 0, 64) }
    end
    assert_raises(ArgumentError) { roster.scan(0, 1.0) }
    assert_raises(NoMethodError) { roster.suspects(0, 1.0) }
    [-1, 2**64].each do |n|
      assert_raises(RangeError) { roster.heartbeat(n, 0, 0) }
      assert_raises(RangeError) { roster.heartbeat(1, n, 0) }
      assert_raises(RangeError) { roster.heartbeat(1, 0, n) }
      assert_raises(RangeError) { roster.scan(n, level(1.0), 0, 64) }
    end
    assert_raises(RangeError) { roster.scan(0, level(1.0), 0, 65) }
    assert_equal 0, roster.size
  end

  def test_configuration_is_explicit_and_validated
    config = Config.new(Normal.new(5), 3)
    assert_equal [Normal.new(5), 3, 0], [config.distribution, config.min_samples, config.acceptable_pause]
    paused = config.with_acceptable_pause(7)
    assert_equal [Normal.new(5), 3, 7], [paused.distribution, paused.min_samples, paused.acceptable_pause]
    assert_equal 0, config.acceptable_pause
    assert_predicate paused, :frozen?
    assert_equal :exponential, Config.new(:exponential, 1).distribution
    assert_raises(ArgumentError) { Config.new }
    assert_raises(ArgumentError) { Config.new(:exponential) }
    assert_raises(ArgumentError) { Config.new(:exponential, 1, 5) }
    assert_raises(ArgumentError) { Config.new(distribution: :exponential, min_samples: 1) }
    assert_raises(RangeError) { Config.new(:weibull, 3) }
    assert_raises(RangeError) { Config.new(:normal, 3) }
    assert_raises(TypeError) { Config.new('normal', 3) }
    assert_raises(TypeError) { Config.new(nil, 3) }
    assert_raises(TypeError) { Config.new(:exponential, 3.0) }
    assert_raises(RangeError) { Config.new(:exponential, 0) }
    assert_raises(TypeError) { config.with_acceptable_pause(nil) }
    assert_raises(TypeError) { config.with_acceptable_pause(1.5) }
    assert_raises(RangeError) { config.with_acceptable_pause(2**64) }
    assert_raises(RangeError) { config.with_acceptable_pause(-1) }
    assert_raises(NoMethodError) { Config.__build(:exponential, 1, 0) }
    assert_raises(NoMethodError) { config.__validate }

    assert_raises(RangeError) { normal(min_samples: 129) }
    assert_instance_of SuspectMachines::Roster, normal(min_samples: 128, floor: 2**64 - 1)
    assert_raises(ArgumentError) { SuspectMachines::Roster.new }
    assert_raises(TypeError) { SuspectMachines::Roster.new(distribution: :exponential, min_samples: 1) }
    assert_raises(TypeError) { SuspectMachines::Roster.new(:exponential) }
    assert_raises(TypeError) { SuspectMachines::Roster.new(nil) }
    assert_raises(TypeError) { SuspectMachines::Roster.allocate }
  end

  def test_freeze_guards_writes_and_keeps_reads
    roster = exponential
    roster.heartbeat(1, 0, 0)
    roster.heartbeat(1, 0, 10)
    roster.freeze
    assert_raises(FrozenError) { roster.heartbeat(1, 0, 20) }
    assert_raises(FrozenError) { roster.forget(1) }
    assert_in_delta 1 / Math.log(10), phi(roster, 1, 20), 1e-12
    assert_equal [1], roster.scan(20, level(0.1), 0, 64).suspects.map(&:peer)
    assert_equal 1, roster.statistics(1).samples
    assert_equal [1, 0, 10], [roster.size, roster.incarnation(1), roster.last_heartbeat(1)]
  end

  def test_u64_limits_and_gc_stress
    previous = GC.stress
    GC.stress = true
    max = 2**64 - 1
    roster = normal(min_samples: 1, floor: 1)
    assert_equal :first, roster.heartbeat(max, max, 0)
    assert_equal Recorded.new(1, 1), roster.heartbeat(max, max, 1)
    level = phi(roster, max, max)
    assert level.finite?
    assert_operator level, :>, 1e37
    suspect = roster.scan(max, level(1e30), 0, 64).suspects.fetch(0)
    assert_equal [max, max], [suspect.peer, suspect.incarnation]
    assert_equal Recorded.new(max - 1, 2), roster.heartbeat(max, max, max)
    assert_equal max - 1, roster.statistics(max).max
    GC.compact
    assert_equal :same_instant, roster.heartbeat(max, max, max)
  ensure
    GC.stress = previous
  end
end
