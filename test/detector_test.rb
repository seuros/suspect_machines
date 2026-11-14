# frozen_string_literal: true
require 'minitest/autorun'
require 'suspect_machines'

class DetectorTest < Minitest::Test
  SECOND = 1_000_000_000
  MILLI = 1_000_000

  include SuspectMachines

  def normal(min_samples = 3) = Detector.new(Config.new(Normal.new(100 * MILLI), min_samples))
  def exponential(min_samples = 1) = Detector.new(Config.new(:exponential, min_samples))

  def test_starts_with_no_heartbeat
    detector = normal
    assert_equal 128, Detector::WINDOW
    assert_equal [128, 0, nil, nil], [detector.window, detector.samples, detector.last_heartbeat, detector.statistics]
    assert_equal Config.new(Normal.new(100 * MILLI), 3), detector.config
    assert_equal :no_heartbeat, detector.suspicion(0)
    assert_equal :no_heartbeat, detector.suspicion(2**64 - 1)
  end

  def test_warm_up_then_phi
    detector = normal
    assert_equal :first, detector.heartbeat(0)
    assert_equal WarmingUp.new(0, 3, 5 * SECOND), detector.suspicion(5 * SECOND)
    assert_equal Recorded.new(SECOND, 1), detector.heartbeat(1 * SECOND)
    assert_equal Recorded.new(SECOND, 2), detector.heartbeat(2 * SECOND)
    assert_equal WarmingUp.new(2, 3, SECOND), detector.suspicion(3 * SECOND)
    assert_equal Recorded.new(SECOND, 3), detector.heartbeat(3 * SECOND)
    assert_equal [3, 3 * SECOND], [detector.samples, detector.last_heartbeat]
    measured = detector.suspicion(4 * SECOND + 300 * MILLI)
    assert_instance_of Measured, measured
    assert_predicate measured, :frozen?
    assert_in_delta 2.869699035929369, measured.phi.value, 1e-9
    assert_equal 2_869, measured.phi.millis
    stats = detector.statistics
    assert_equal [3, 1e9, 0.0, SECOND, SECOND], [stats.samples, stats.mean_ns, stats.std_dev_ns, stats.min, stats.max]
  end

  def test_refusals_change_nothing
    detector = exponential
    detector.heartbeat(100)
    detector.heartbeat(200)
    before = [detector.samples, detector.last_heartbeat, detector.statistics]
    assert_equal :same_instant, detector.heartbeat(200)
    assert_equal :time_regression, detector.heartbeat(199)
    assert_equal :time_regression, detector.suspicion(199)
    assert_equal before, [detector.samples, detector.last_heartbeat, detector.statistics]
    assert_in_delta 1 / Math.log(10), detector.suspicion(300).phi.value, 1e-12
  end

  def test_reset_starts_the_warm_up_over
    detector = exponential
    [0, 10, 20].each { detector.heartbeat(it) }
    assert_nil detector.reset
    assert_equal [0, nil, nil, :no_heartbeat],
                 [detector.samples, detector.last_heartbeat, detector.statistics, detector.suspicion(30)]
    assert_equal :first, detector.heartbeat(5)
    assert_equal Config.new(:exponential, 1), detector.config
  end

  def test_window_slides
    detector = exponential
    now = 0
    detector.heartbeat(now)
    detector.heartbeat(now += 1000)
    (Detector::WINDOW + 5).times { detector.heartbeat(now += 7) }
    assert_equal [128, 7, 7], [detector.samples, detector.statistics.min, detector.statistics.max]
  end

  def test_strict_inputs_and_configuration
    detector = exponential
    [1.0, Float::NAN, nil, '1', Object.new].each do |value|
      assert_raises(TypeError) { detector.heartbeat(value) }
      assert_raises(TypeError) { detector.suspicion(value) }
      assert_raises(TypeError) { Detector.new(value) }
    end
    [-1, 2**64].each do |n|
      assert_raises(RangeError) { detector.heartbeat(n) }
      assert_raises(RangeError) { detector.suspicion(n) }
    end
    assert_raises(RangeError) { exponential(129) }
    assert_instance_of Detector, exponential(128)
    assert_raises(ArgumentError) { Detector.new }
    assert_raises(TypeError) { Detector.new(:exponential) }
    assert_raises(TypeError) { Detector.allocate }
    assert_equal 0, detector.samples
  end

  def test_freeze_guards_writes_and_keeps_reads
    detector = exponential
    detector.heartbeat(0)
    detector.heartbeat(10)
    detector.freeze
    assert_raises(FrozenError) { detector.heartbeat(20) }
    assert_raises(FrozenError) { detector.reset }
    # Frozen is checked before any argument is read.
    assert_raises(FrozenError) { detector.heartbeat(1.5) }
    assert_in_delta 1 / Math.log(10), detector.suspicion(20).phi.value, 1e-12
    assert_equal [1, 10], [detector.samples, detector.last_heartbeat]
  end
end
