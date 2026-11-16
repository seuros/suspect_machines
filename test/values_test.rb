# frozen_string_literal: true
require 'minitest/autorun'
require 'suspect_machines'

# Every Data value is checked by the core when built, by the gem or by hand.
class ValuesTest < Minitest::Test
  include SuspectMachines

  MAX = 2**64 - 1

  def phi(value) = Phi.new(value)

  def test_phi_and_threshold_mirror_the_core
    assert_equal 1.0, phi(1).value
    assert_instance_of Float, phi(1).value
    assert_equal 0.0, phi(-0.0).value
    assert_equal [2_500, MAX], [phi(2.5).millis, phi(1e300).millis]
    assert phi(3).reaches?(Threshold.new(3))
    refute phi(3).reaches?(Threshold.new(3.5))
    assert_equal 0.5, Threshold.new(0.5).value
    [-1e-300, -1, Float::NAN, Float::INFINITY].each { |bad| assert_raises(RangeError) { phi(bad) } }
    [0, 0.0, -1, Float::NAN, -Float::INFINITY].each { |bad| assert_raises(RangeError) { Threshold.new(bad) } }
    [nil, '1', :one, Object.new].each do |bad|
      assert_raises(TypeError) { phi(bad) }
      assert_raises(TypeError) { Threshold.new(bad) }
    end
    [1.0, 3, phi(3), nil].each { |bad| assert_raises(TypeError) { phi(3).reaches?(bad) } }
    assert_raises(RangeError) { phi(1).with(value: -1) }
  end

  # Levels are exactly a Float or an Integer, read natively: a Rational or
  # an object answering to_f is TypeError, no to_f runs (a redefined
  # Integer#to_f included), and a non-finite level is RangeError before
  # the core sees it, an Integer past Float::MAX included.
  def test_levels_are_finite_floats_or_integers_read_natively
    to_f = Object.new.tap { |object| def object.to_f = 1.0 }
    [1r, to_f, Complex(1, 0)].each do |bad|
      error = assert_raises(TypeError) { phi(bad) }
      assert_equal 'phi must be a Float or Integer', error.message
      error = assert_raises(TypeError) { Threshold.new(bad) }
      assert_equal 'threshold must be a Float or Integer', error.message
      error = assert_raises(TypeError) { Statistics.new(1, bad, 0, 1, 1) }
      assert_equal 'mean_ns must be a Float or Integer', error.message
      error = assert_raises(TypeError) { Statistics.new(1, 1, bad, 1, 1) }
      assert_equal 'std_dev_ns must be a Float or Integer', error.message
    end
    [Float::NAN, Float::INFINITY, -Float::INFINITY, 2**1024].each do |bad|
      assert_equal 'phi must be finite', assert_raises(RangeError) { phi(bad) }.message
      assert_equal 'threshold must be finite', assert_raises(RangeError) { Threshold.new(bad) }.message
      assert_equal 'mean_ns must be finite', assert_raises(RangeError) { Statistics.new(1, bad, 0, 1, 1) }.message
      assert_equal 'std_dev_ns must be finite', assert_raises(RangeError) { Statistics.new(1, 1, bad, 1, 1) }.message
    end
    redefining(Integer, to_f: proc { raise 'Integer#to_f called' }) do
      assert_equal 3.0, phi(3).value
      assert_equal 2.0, Threshold.new(2).value
      assert_equal [15.0, 5.0], Statistics.new(2, 15, 5, 10, 20).then { [it.mean_ns, it.std_dev_ns] }
    end
  end

  def test_answer_values_reject_what_the_core_never_answers
    assert_equal [5, 128], Recorded.new(5, 128).to_h.values
    assert_raises(RangeError) { Recorded.new(0, 1) }
    assert_raises(RangeError) { Recorded.new(1, 0) }
    assert_raises(RangeError) { Recorded.new(1, 129) }
    assert_raises(TypeError) { Recorded.new(1.0, 1) }
    assert_equal [0, 1, 0], WarmingUp.new(0, 1, 0).to_h.values
    assert_raises(RangeError) { WarmingUp.new(3, 3, 0) }
    assert_raises(RangeError) { WarmingUp.new(0, 0, 0) }
    assert_raises(RangeError) { WarmingUp.new(0, 129, 0) }
    assert_raises(RangeError) { WarmingUp.new(0, 1, -1) }
    assert_equal phi(1), Measured.new(phi(1)).phi
    [1.0, 1, nil, Threshold.new(1)].each { |bad| assert_raises(TypeError) { Measured.new(bad) } }
  end

  def test_statistics_suspect_and_scan_reject_what_the_core_never_answers
    stats = Statistics.new(2, 15, 5, 10, 20)
    assert_equal [2, 15.0, 5.0, 10, 20], stats.to_h.values
    assert_instance_of Float, stats.mean_ns
    assert_raises(RangeError) { Statistics.new(0, 1.0, 0.0, 1, 1) }
    assert_raises(RangeError) { Statistics.new(129, 1.0, 0.0, 1, 1) }
    assert_raises(RangeError) { Statistics.new(1, -1.0, 0.0, 1, 1) }
    assert_raises(RangeError) { Statistics.new(1, 1.0, Float::NAN, 1, 1) }
    assert_raises(RangeError) { Statistics.new(1, 1.0, 0.0, 0, 1) }
    assert_raises(RangeError) { Statistics.new(1, 1.0, 0.0, 2, 1) }
    assert_raises(TypeError) { Statistics.new(1, '1', 0.0, 1, 1) }
    # Only what a window can report: the mean within min..max, a positively
    # signed deviation of at most half the range, one sample its own min and max.
    assert_raises(RangeError) { Statistics.new(2, 5.0, 0.0, 10, 20) }
    assert_raises(RangeError) { Statistics.new(2, 25.0, 0.0, 10, 20) }
    assert_raises(RangeError) { Statistics.new(1, -0.0, -0.0, 1, 1) }
    assert_raises(RangeError) { Statistics.new(1, 1.0, -0.0, 1, 1) }
    assert_raises(RangeError) { Statistics.new(2, 15.0, 5.5, 10, 20) }
    assert_raises(RangeError) { Statistics.new(1, 15.0, 0.0, 10, 20) }
    assert_equal 0.0, Statistics.new(1, 7, 0, 7, 7).std_dev_ns

    suspect = Suspect.new(MAX, 0, Measured.new(phi(4)))
    assert_equal [MAX, 0, Measured.new(phi(4))], suspect.to_h.values
    silent = Suspect.new(1, 0, WarmingUp.new(0, 3, 9))
    assert_equal WarmingUp.new(0, 3, 9), silent.suspicion
    assert_raises(TypeError) { Suspect.new(1, 0, 4.0) }
    assert_raises(TypeError) { Suspect.new(1, 0, phi(4)) }
    assert_raises(TypeError) { Suspect.new(1, 0, :no_heartbeat) }
    assert_raises(RangeError) { Suspect.new(-1, 0, Measured.new(phi(4))) }

    list = [suspect]
    scan = Scan.new(list, 63)
    assert_equal [[suspect], 63], [scan.suspects, scan.next]
    assert_predicate scan.suspects, :frozen?
    list << suspect
    assert_equal 1, scan.suspects.size
    assert_nil Scan.new([], nil).next
    assert_raises(RangeError) { Scan.new([], 64) }
    assert_raises(TypeError) { Scan.new(Class.new(Array).new, nil) } # an Array subclass
    assert_raises(RangeError) { Scan.new([suspect] * 65, nil) }
    assert_raises(TypeError) { Scan.new([1], nil) }
    assert_raises(TypeError) { Scan.new(nil, nil) }
    assert_raises(TypeError) { Scan.new([], 1.0) }
  end

  # The defect: `Normal.new(0)` was accepted and only `Config` refused it.
  def test_configuration_values_check_through_the_core
    assert_equal MAX, Normal.new(MAX).min_std_dev
    [0, -1, 2**64].each { |bad| assert_raises(RangeError) { Normal.new(bad) } }
    [1.0, nil, '1'].each { |bad| assert_raises(TypeError) { Normal.new(bad) } }
    assert_raises(RangeError) { Normal.new(1).with(min_std_dev: 0) }
    assert_raises(RangeError) { Config.new(:exponential, 1).with(min_samples: 0) }
    assert_raises(NoMethodError) { SuspectMachines.__phi(1) }
    assert_raises(NoMethodError) { SuspectMachines.__scan([], nil) }
  end

  # The defect: a subclass overriding `initialize`, or `allocate`, built a
  # value that never ran the core's check.
  def test_values_are_final
    [Phi, Threshold, Normal, Config, Recorded, WarmingUp, Measured, Statistics, Suspect, Scan].each do |value|
      assert_raises(TypeError) { Class.new(value) { def initialize(**) = nil } }
      assert_raises(NoMethodError) { value.allocate }
    end
  end

  def exponential = Detector.new(Config.new(:exponential, 1))

  # Sealing stops accidents, not reflection: a named subclass raises, yet
  # Ruby has already bound its constant, and `Data#initialize` can be bound
  # to skip the validating one. The binding refuses such a value by its
  # exact class before it reaches the core.
  def test_a_leaked_subclass_never_reaches_the_core
    %w[ForgedConfig<Config ForgedThreshold<Threshold ForgedPhi<Phi].each do |pair|
      name, parent = pair.split('<')
      assert_raises(TypeError) { eval("class #{name} < SuspectMachines::#{parent}; end", binding, __FILE__, __LINE__) }
    end
    assert_equal 'constant', defined?(ForgedConfig) # the constant leaks
    [ForgedConfig, ForgedThreshold, ForgedPhi].each do |forgery|
      forgery.define_method(:initialize) { |**kw| Data.instance_method(:initialize).bind_call(self, **kw) }
    end
    assert_raises(TypeError) { Detector.new(ForgedConfig.send(:__build, :exponential, 0, 0, nil)) }
    roster = Roster.new(Config.new(:exponential, 1))
    assert_raises(TypeError) { roster.scan(0, ForgedThreshold.new(value: -1.0), 0, 1) }
    assert_raises(TypeError) { Measured.new(ForgedPhi.new(value: -1.0)) }
    assert_raises(TypeError) { phi(1).reaches?(ForgedThreshold.new(value: 0.5)) }
    error = assert_raises(TypeError) { ForgedConfig.send(:__build, :exponential, 1, 0, nil).with_acceptable_pause(5) }
    assert_equal 'expected a SuspectMachines::Config', error.message
  ensure
    %i[ForgedConfig ForgedThreshold ForgedPhi].each do |name|
      self.class.send(:remove_const, name) if self.class.const_defined?(name, false)
    end
  end

  # `Marshal.load` builds an instance of the real class without `initialize`;
  # the binding rebuilds every value it reads through the core.
  # A singleton class opened on a value does not change what it is.
  def test_an_opened_singleton_class_is_still_the_gems_value
    config = Config.new(Normal.new(1).tap(&:singleton_class), 1).tap(&:singleton_class)
    assert_equal :first, Detector.new(config).heartbeat(0)
  end

  # Fields are read from the struct itself: a redefined reader is never
  # called, so it can neither lie nor re-enter.
  def test_a_redefined_reader_is_ignored
    config = Config.new(Normal.new(1), 1)
    level = Threshold.new(0.5)
    listed = [Suspect.new(1, 0, Measured.new(phi(1.0))), Suspect.new(2, 0, WarmingUp.new(0, 1, 5))]
    readers = { Config => %i[distribution min_samples acceptable_pause warm_up_ceiling], Normal => %i[min_std_dev],
                Threshold => %i[value], Phi => %i[value], Suspect => %i[peer incarnation suspicion],
                Measured => %i[phi], WarmingUp => %i[samples required silence] }
    redefined(readers) do
      assert_equal :first, Detector.new(config).heartbeat(0)
      roster = Roster.new(config)
      assert_equal Scan.new([], nil), roster.scan(0, level, 0, 1)
      assert_equal 2, Scan.new(listed, nil).suspects.size
      assert phi(1.0).reaches?(level)
      assert_equal Config.send(:__build, Normal.new(1), 1, 7, nil), config.with_acceptable_pause(7)
      assert_equal Config.send(:__build, Normal.new(1), 1, 0, 3), config.with_warm_up_ceiling(3)
    end
  end

  def redefined(readers)
    originals = readers.flat_map { |klass, names| names.map { [klass, it, klass.instance_method(it)] } }
    originals.each { |klass, name, _| replace_method(klass, name, proc { raise "#{name} reader called" }) }
    yield
  ensure
    originals&.each { |klass, name, method| replace_method(klass, name, (method if method.owner == klass)) }
  end

  def test_a_marshal_forged_value_never_reaches_the_core
    assert_raises(RangeError) { Detector.new(forged(Config, :exponential, 0, 0, nil)) }
    assert_raises(RangeError) { Detector.new(Config.new(forged(Normal, 0), 1)) }
    assert_raises(RangeError) { forged(Config, :exponential, 0, 0, nil).with_acceptable_pause(5) }
    assert_raises(RangeError) { forged(Config, forged(Normal, 0), 1, 0, nil).with_acceptable_pause(5) }
    assert_raises(RangeError) { Detector.new(forged(Config, :exponential, 1, 0, -1)) }
    assert_raises(TypeError) { forged(Config, :exponential, 1, 0, 1.5).with_warm_up_ceiling(5) }
    roster = Roster.new(Config.new(:exponential, 1))
    assert_raises(RangeError) { roster.scan(0, forged(Threshold, -1.0), 0, 1) }
    assert_raises(RangeError) { Measured.new(forged(Phi, Float::NAN)) }
    assert_raises(TypeError) { Scan.new([forged(Suspect, 1, 0, forged(Phi, -1.0))], nil) }
    assert_raises(RangeError) { Scan.new([forged(Suspect, 1, 0, forged(Measured, forged(Phi, -1.0)))], nil) }
    assert_raises(RangeError) { Scan.new([forged(Suspect, 1, 0, forged(WarmingUp, 3, 3, 1))], nil) }
    assert_equal 0, roster.size
  end

  def test_heartbeat_applies_nothing_when_building_its_answer_raises_re_enters_or_freezes
    [
      [IOError, ->(_) { raise IOError }],
      [RuntimeError, ->(detector) { detector.reset }],
      [FrozenError, lambda(&:freeze)]
    ].each do |error, hook|
      detector = exponential
      detector.heartbeat(0)
      replacing(Recorded, ->(*) { hook.call(detector) }) { assert_raises(error) { detector.heartbeat(10) } }
      expected = error == RuntimeError ? [0, nil] : [0, 0]
      assert_equal expected, [detector.samples, detector.last_heartbeat]
    end
  end

  def test_roster_heartbeat_applies_nothing_when_building_its_answer_raises_re_enters_or_freezes
    [
      [IOError, ->(_) { raise IOError }],
      [RuntimeError, ->(roster) { roster.forget(1) }],
      [FrozenError, lambda(&:freeze)]
    ].each do |error, hook|
      roster = Roster.new(Config.new(:exponential, 1))
      roster.heartbeat(1, 0, 0)
      replacing(Recorded, ->(*) { hook.call(roster) }) { assert_raises(error) { roster.heartbeat(1, 0, 10) } }
      expected = error == RuntimeError ? [0, nil] : [1, 0]
      assert_equal expected, [roster.size, roster.last_heartbeat(1)]
    end
  end

  # No borrow is held while a reader builds its answer, so a re-entrant call
  # from `new` works instead of aborting the process.
  def test_readers_hold_no_borrow_while_building
    detector = Detector.new(Config.new(Normal.new(1), 1))
    roster = Roster.new(Config.new(Normal.new(1), 1))
    [0, 10, 25].each do |now|
      detector.heartbeat(now)
      roster.heartbeat(1, 0, now)
    end
    replacing(Normal, ->(*) { detector.heartbeat(30) }) { detector.config }
    replacing(Statistics, ->(*) { detector.heartbeat(40) }) { detector.statistics }
    replacing(Measured, ->(*) { detector.heartbeat(50) }) { detector.suspicion(60) }
    replacing(Normal, ->(*) { roster.heartbeat(1, 0, 30) }) { roster.config }
    replacing(Statistics, ->(*) { roster.heartbeat(1, 0, 40) }) { roster.statistics(1) }
    replacing(Scan, ->(*) { roster.heartbeat(1, 0, 50) }) { roster.scan(10**12, Threshold.new(0.1), 0, 1) }
    assert_equal [50, 50], [detector.last_heartbeat, roster.last_heartbeat(1)]
  end

  # The classes are resolved once and pinned: after a compaction that moves
  # everything movable, every path that checks or builds a value still finds
  # them (unpinned, the exact-class check would refuse the gem's own values).
  def test_classes_survive_gc_compaction
    if GC.respond_to?(:verify_compaction_references)
      GC.verify_compaction_references(expand_heap: true, toward: :empty)
    elsif GC.respond_to?(:compact)
      GC.compact
    end
    detector = Detector.new(Config.new(Normal.new(1), 2))
    detector.heartbeat(0)
    assert_instance_of Recorded, detector.heartbeat(10)
    assert_instance_of WarmingUp, detector.suspicion(20)
    detector.heartbeat(25)
    assert_instance_of Measured, detector.suspicion(30)
    assert_instance_of Phi, detector.suspicion(30).phi
    assert_instance_of Statistics, detector.statistics
    assert_instance_of Normal, detector.config.distribution
    assert_instance_of Config, detector.config
    roster = Roster.new(Config.new(:exponential, 1))
    roster.heartbeat(1, 0, 0)
    assert_instance_of Recorded, roster.heartbeat(1, 0, 10)
    scan = roster.scan(10**12, Threshold.new(0.1), 0, 1)
    assert_instance_of Scan, scan
    assert_instance_of Suspect, scan.suspects.first
    assert phi(3).reaches?(Threshold.new(3))
  end

  private

  # Runs the block with `klass.new` calling `hook` first, once; the original
  # `new` is restored afterwards.
  def replacing(klass, hook, &)
    original = klass.method(:new)
    armed = true
    body = proc do |*args, **kw|
      if armed
        armed = false
        hook.call(*args)
      end
      original.call(*args, **kw)
    end
    redefining(klass.singleton_class, new: body, &)
  end

  # Runs the block with each method of `klass` named in `bodies` replaced by
  # its body, then restores the originals.
  def redefining(klass, **bodies)
    originals = bodies.to_h { |name, _| [name, klass.instance_method(name)] }
    bodies.each { |name, body| replace_method(klass, name, body) }
    yield
  ensure
    originals&.each { |name, method| replace_method(klass, name, (method if method.owner == klass)) }
  end

  # Defines `name` on `klass` as `body` (removes it when `body` is nil),
  # removing the method `klass` itself holds first. Ruby warns about the
  # redefinition, deliberate here, so warnings are off for the swap alone.
  def replace_method(klass, name, body)
    verbose = $VERBOSE
    $VERBOSE = nil
    klass.remove_method(name) if klass.method_defined?(name, false) || klass.private_method_defined?(name, false)
    klass.define_method(name, body) if body
  ensure
    $VERBOSE = verbose
  end

  # An instance of `klass` holding `fields`, loaded by Marshal from a look-alike
  # so its `initialize` never ran.
  def forged(klass, *fields)
    stand_in = Data.define(*klass.members)
    Object.const_set(:MarshalStandIn, stand_in)
    bytes = Marshal.dump(stand_in.new(*fields))
    name = klass.name
    Marshal.load(bytes.sub(":#{(5 + 14).chr}MarshalStandIn".b, ":#{(5 + name.size).chr}#{name}".b))
  ensure
    Object.send(:remove_const, :MarshalStandIn)
  end
end
