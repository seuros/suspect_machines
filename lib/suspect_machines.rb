# frozen_string_literal: true

require_relative 'suspect_machines/version'

# Every time, instant or span, is Integer nanoseconds. Every value mirrors the
# Rust core's type of the same name, field for field, and is checked by the
# core when it is built, whether by the gem or by hand: a wrong type raises
# TypeError, a value the core would never hold raises RangeError.
module SuspectMachines
  # The core's `Phi`: a suspicion level, finite and not negative (a Float).
  # `millis` and `reaches?` are the core's `Phi::millis` and `Phi::reaches`.
  Phi = Data.define(:value) do
    def initialize(value:)
      super(value: SuspectMachines.__send__(:__phi, value))
    end
  end

  # The core's `Threshold`: a phi level worth acting on, finite and above
  # zero (a Float).
  Threshold = Data.define(:value) do
    def initialize(value:)
      super(value: SuspectMachines.__send__(:__threshold, value))
    end
  end

  # The core's `Normal`, carried by `Distribution::Normal`: intervals are
  # assumed normally distributed, with the standard deviation floored at
  # `min_std_dev`, at least 1 (`Normal::new` refuses zero: RangeError). The
  # other distribution, `Distribution::Exponential`, is the symbol :exponential.
  Normal = Data.define(:min_std_dev) do
    def initialize(min_std_dev:)
      super(min_std_dev: SuspectMachines.__send__(:__normal, min_std_dev))
    end
  end

  # Detector policy. `Config.new(distribution, min_samples)` is the core's
  # constructor and tolerates no pause and no warm-up ceiling;
  # `with_acceptable_pause` and `with_warm_up_ceiling` answer a copy with one.
  # `distribution` is :exponential or a Normal; `warm_up_ceiling` is nil or
  # Integer nanoseconds.
  Config = Data.define(:distribution, :min_samples, :acceptable_pause, :warm_up_ceiling) do
    class << self
      alias_method :__build, :new
      private :__build

      def new(distribution, min_samples) = __build(distribution, min_samples, 0, nil)
    end

    def initialize(distribution:, min_samples:, acceptable_pause:, warm_up_ceiling:)
      distribution, min_samples, acceptable_pause, warm_up_ceiling =
        SuspectMachines.__send__(:__config, distribution, min_samples, acceptable_pause, warm_up_ceiling)
      super
    end

    # `with_acceptable_pause(pause)` and `with_warm_up_ceiling(ceiling)` are
    # native: the core's `Config::with_acceptable_pause` and
    # `Config::with_warm_up_ceiling` on this config, read from the struct by
    # its exact class (never through the readers) and checked again.
  end

  # The core's `Beat::Recorded`: an accepted heartbeat that put an interval in
  # the window, the time since the previous heartbeat and the intervals now held.
  Recorded = Data.define(:interval, :samples) do
    def initialize(interval:, samples:)
      interval, samples = SuspectMachines.__send__(:__recorded, interval, samples)
      super
    end
  end

  # The core's `Suspicion::WarmingUp`: fewer intervals than the warm-up
  # requires, so no phi exists yet. `silence` is the time since the last
  # heartbeat, for a bootstrap timeout.
  WarmingUp = Data.define(:samples, :required, :silence) do
    def initialize(samples:, required:, silence:)
      samples, required, silence = SuspectMachines.__send__(:__warming_up, samples, required, silence)
      super
    end
  end

  # The core's `Suspicion::Measured`: the suspicion level, a Phi.
  Measured = Data.define(:phi) do
    def initialize(phi:)
      super(phi: SuspectMachines.__send__(:__measured, phi))
    end
  end

  # Statistics of one peer's interval window. The mean and the population
  # standard deviation (before any floor) are Float nanoseconds; `min` and
  # `max` are the shortest and longest interval.
  Statistics = Data.define(:samples, :mean_ns, :std_dev_ns, :min, :max) do
    def initialize(samples:, mean_ns:, std_dev_ns:, min:, max:)
      samples, mean_ns, std_dev_ns, min, max =
        SuspectMachines.__send__(:__statistics, samples, mean_ns, std_dev_ns, min, max)
      super
    end
  end

  # A peer a scan lists: `suspicion` is a Measured whose phi reached the
  # threshold, or a WarmingUp silent past the config's warm-up ceiling.
  Suspect = Data.define(:peer, :incarnation, :suspicion) do
    def initialize(peer:, incarnation:, suspicion:)
      peer, incarnation, suspicion = SuspectMachines.__send__(:__suspect, peer, incarnation, suspicion)
      super
    end
  end

  # One bounded page of suspects (a frozen Array). The core's `Scan::suspects`
  # counts the suspects it wrote to the caller's buffer; here `suspects` holds
  # them. `next` is the cursor to resume from, nil once the roster has been
  # fully examined.
  Scan = Data.define(:suspects, :next) do
    def initialize(suspects:, next:)
      suspects, cursor = SuspectMachines.__send__(:__scan, suspects, binding.local_variable_get(:next))
      super(suspects:, next: cursor)
    end
  end
end

# Native is required: no algorithm fallback. It checks and builds the Data
# values above, so they must exist first.
require 'suspect_machines_native/suspect_machines_native'

module SuspectMachines
  private_class_method :__phi, :__threshold, :__normal, :__config, :__recorded, :__warming_up,
                       :__measured, :__statistics, :__suspect, :__scan

  # The values are final. `initialize` is where each validates, and a subclass
  # could override it, or `allocate` build an instance that never ran it:
  # subclassing raises TypeError (as subclassing Class does) and `allocate`
  # is gone. `new`, keywords, `with` and `dup` all still run the check.
  [Phi, Threshold, Normal, Config, Recorded, WarmingUp, Measured, Statistics, Suspect, Scan].each do |value|
    value.singleton_class.undef_method(:allocate)
    value.define_singleton_method(:inherited) { |_| raise TypeError, "#{value} cannot be subclassed" }
  end
end
