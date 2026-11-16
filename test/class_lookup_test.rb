# frozen_string_literal: true

require 'minitest/autorun'
require 'open3'
require 'rbconfig'

# The classes of the values the extension wraps (Detector, Roster) are fixed
# when it loads. Nothing a caller does later, redefining a constant or
# `Object.const_get`, reaches a value being built. Each case runs in a fresh
# process: the class is fixed once per process, at load.
class ClassLookupTest < Minitest::Test
  LIB = File.expand_path('../lib', __dir__)

  PRELUDE = <<~RUBY
    require 'suspect_machines'
    S = SuspectMachines
    CONFIG = S::Config.new(S::Normal.new(100_000_000), 3)
  RUBY

  # Runs `body` after PRELUDE in a new Ruby; answers its last line of output.
  def isolated(body)
    out, err, status = Open3.capture3(RbConfig.ruby, '-I', LIB, '-e', PRELUDE + body)
    assert status.success?, "exited #{status.exitstatus}: #{err}"
    out.lines.last&.chomp
  end

  # Bug: a Detector constant redefined before the first detector made it an
  # instance of the new class, without any of the detector's methods.
  def test_a_redefined_detector_constant_does_not_change_the_detector_class
    assert_equal 'true first 0', isolated(<<~RUBY)
      original = S::Detector
      S.send(:remove_const, :Detector)
      S.const_set(:Detector, Class.new)
      detector = original.new(CONFIG)
      puts [detector.instance_of?(original), detector.heartbeat(0), detector.last_heartbeat].join(' ')
    RUBY
  end

  # Bug: the same for a Roster.
  def test_a_redefined_roster_constant_does_not_change_the_roster_class
    assert_equal 'true first 1', isolated(<<~RUBY)
      original = S::Roster
      S.send(:remove_const, :Roster)
      S.const_set(:Roster, Class.new)
      roster = original.new(CONFIG)
      puts [roster.instance_of?(original), roster.heartbeat(7, 1, 0), roster.size].join(' ')
    RUBY
  end

  # Bug: building the first value ran `Object.const_get`, a caller's Ruby code
  # in the middle of a native call.
  def test_building_a_value_runs_no_const_get
    assert_equal '0 Detector Roster', isolated(<<~RUBY)
      hits = 0
      Object.define_singleton_method(:const_get) do |*args|
        hits += 1
        super(*args)
      end
      values = [S::Detector.new(CONFIG), S::Roster.new(CONFIG)]
      puts [hits, *values.map { |value| value.class.name.split('::').last }].join(' ')
    RUBY
  end

  # Bug: a redefined `Object.const_get` that raised hit an `unwrap` in the
  # extension: an uncatchable fatal, and the process exited.
  def test_a_raising_const_get_does_not_kill_the_process
    assert_equal 'first first', isolated(<<~RUBY)
      Object.define_singleton_method(:const_get) { |*| raise IOError, 'lookup ran mid-call' }
      puts [S::Detector.new(CONFIG).heartbeat(0), S::Roster.new(CONFIG).heartbeat(7, 1, 0)].join(' ')
    RUBY
  end
end
