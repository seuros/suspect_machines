# frozen_string_literal: true

require_relative 'suspect_machines/version'

# Every time, instant or span, is Integer nanoseconds. Every value mirrors the
# Rust core's type of the same name, field for field.
module SuspectMachines
  # The core's `Distribution::Normal`: intervals are assumed normally
  # distributed, with the standard deviation floored at `min_std_dev`. The
  # other distribution, `Distribution::Exponential`, is the symbol :exponential.
  Normal = Data.define(:min_std_dev) do
    def initialize(min_std_dev:)
      super
      __validate
    end
  end

  # Detector policy. `Config.new(distribution, min_samples)` is the core's
  # constructor and tolerates no pause; `with_acceptable_pause` answers a copy
  # that does. `distribution` is :exponential or a Normal.
  Config = Data.define(:distribution, :min_samples, :acceptable_pause) do
    class << self
      alias_method :__build, :new
      private :__build

      def new(distribution, min_samples) = __build(distribution, min_samples, 0)
    end

    def initialize(distribution:, min_samples:, acceptable_pause:)
      super
      __validate
    end

    def with_acceptable_pause(pause) = self.class.send(:__build, distribution, min_samples, pause)
  end

  # The core's `Beat::Recorded`: an accepted heartbeat that put an interval in
  # the window, the time since the previous heartbeat and the intervals now held.
  Recorded = Data.define(:interval, :samples)

  # The core's `Suspicion::WarmingUp`: fewer intervals than the warm-up
  # requires, so no phi exists yet. `silence` is the time since the last
  # heartbeat, for a bootstrap timeout.
  WarmingUp = Data.define(:samples, :required, :silence)

  # The core's `Suspicion::Measured`: the suspicion level `phi`, a Float.
  Measured = Data.define(:phi)

  # Statistics of one peer's interval window. The mean and the population
  # standard deviation (before any floor) are Float nanoseconds; `min` and
  # `max` are the shortest and longest interval.
  Statistics = Data.define(:samples, :mean_ns, :std_dev_ns, :min, :max)

  # A peer whose phi reached the scan threshold.
  Suspect = Data.define(:peer, :incarnation, :phi)

  # One bounded page of suspects. `next` is the cursor to resume from, nil
  # once the roster has been fully examined.
  Scan = Data.define(:suspects, :next)
end

# Native is required: no algorithm fallback. It builds and checks the Data
# values above, so they must exist first.
require 'suspect_machines_native/suspect_machines_native'
