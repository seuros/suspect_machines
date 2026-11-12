# frozen_string_literal: true
require_relative 'lib/suspect_machines/version'

Gem::Specification.new do |spec|
  spec.name = 'suspect_machines'
  spec.version = SuspectMachines::VERSION
  spec.authors = ['Abdelkader Boudih']
  spec.email = ['oss@seuros.com']
  spec.summary = 'Initial bounded phi-accrual heartbeat suspicion with a native Rust core'
  spec.description = 'Local unstable heartbeat statistics and advisory phi-accrual suspicion; not an authority.'
  spec.homepage = 'https://github.com/seuros/suspect_machines'
  spec.license = 'MIT'
  spec.required_ruby_version = '>= 4.0.0'
  spec.metadata['source_code_uri'] = spec.homepage
  spec.metadata['rubygems_mfa_required'] = 'true'
  spec.metadata['cargo_crate_name'] = 'suspect_machines_native'
  spec.metadata['cargo_manifest_path'] = 'ext/suspect_machines_native/ffi/Cargo.toml'
  spec.files = Dir.glob(%w[
    lib/**/*.rb sig/**/*.rbs ext/**/*.{rs,rb,toml,md} Cargo.toml
    Cargo.lock README.md CHANGELOG.md LICENSE.txt
  ]).select { |p| File.file?(p) }
    .reject { |p| p.end_with?('/tests.rs') || p.include?('/tests/') }
  spec.require_paths = ['lib']
  spec.extensions = ['ext/suspect_machines_native/extconf.rb']
  spec.add_dependency 'rb_sys', '~> 0.9', '>= 0.9.130'
end
