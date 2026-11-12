# frozen_string_literal: true

require_relative 'suspect_machines/version'

module SuspectMachines
  # Statistics of one peer's interval window. Times are nanoseconds; the
  # standard deviation is the population one, before any floor.
  Statistics = Data.define(:samples, :mean_ns, :std_dev_ns, :min_ns, :max_ns)
  # A peer whose phi reached the scan threshold.
  Suspect = Data.define(:peer, :incarnation, :phi)
  # One bounded page of suspects. `next_cursor` is nil once the roster has
  # been fully examined.
  Scan = Data.define(:suspects, :next_cursor)
end

# Native is required: no algorithm fallback. It builds the Data values above,
# so they must exist first.
require 'suspect_machines_native/suspect_machines_native'
require_relative 'suspect_machines/roster'
