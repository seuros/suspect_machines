# frozen_string_literal: true

module SuspectMachines
  # Keyword front for the native roster. The algorithm lives in the extension.
  class Roster
    class << self
      # The distribution and the warm-up are assumptions the caller states;
      # neither has a default.
      def new(distribution:, min_samples:, min_std_dev_ns: nil, acceptable_pause_ns: 0)
        raise TypeError, 'distribution must be a Symbol' unless distribution.is_a?(Symbol)

        case distribution
        when :normal
          raise ArgumentError, ':normal requires min_std_dev_ns' if min_std_dev_ns.nil?
        when :exponential
          raise ArgumentError, ':exponential uses only the mean; drop min_std_dev_ns' unless min_std_dev_ns.nil?

          min_std_dev_ns = 0
        else
          raise ArgumentError, 'distribution must be :normal or :exponential'
        end
        _new(distribution, min_samples, min_std_dev_ns, acceptable_pause_ns)
      end

      private :_new
    end

    def suspects(now_ns, threshold, cursor: 0, limit: CAPACITY)
      _suspects(now_ns, threshold, cursor, limit)
    end

    private :_suspects
  end
end
