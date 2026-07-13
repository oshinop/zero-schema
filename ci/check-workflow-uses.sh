#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
workflows=$root/.github/workflows
[ -d "$workflows" ] || { echo "missing workflow directory: $workflows" >&2; exit 1; }
command -v ruby >/dev/null 2>&1 || { echo 'ruby is required for YAML-aware workflow inspection' >&2; exit 1; }

ruby - "$workflows" <<'RUBY'
require "yaml"
dir = ARGV.fetch(0)
allowed = [
  "actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683",
  "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02",
  "./.github/workflows/ci.yml",
  "./.github/workflows/miri.yml",
  "./.github/workflows/fuzz.yml",
].freeze
found_gate = false
required_test_callers = {
  "ci/test-run-fuzz.sh" => false,
}
errors = []
upload_action = "actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02"
walk = lambda do |node, file|
  case node
  when Hash
    node.each do |key, value|
      if key.to_s == "uses"
        unless value.is_a?(String)
          errors << "#{file}: uses value is not a string"
          next
        end
        errors << "#{file}: disallowed uses: #{value}" unless allowed.include?(value)
        errors << "#{file}: setup/cache actions are forbidden: #{value}" if value.match?(/setup|cache/i)
        if value == upload_action
          inputs = node["with"] || node[:with]
          unless inputs.is_a?(Hash)
            errors << "#{file}: upload-artifact step is missing with inputs"
            next
          end
          name = inputs["name"] || inputs[:name]
          errors << "#{file}: upload-artifact name must be a nonempty string" unless name.is_a?(String) && !name.empty?
          errors << "#{file}: upload-artifact #{name || "<unnamed>"} requires literal overwrite: true" unless inputs["overwrite"] == true || inputs[:overwrite] == true
        end
      end
      if key.to_s == "run" && value.is_a?(String)
        found_gate = true if value.match?(%r{(?:^|[\s/])ci/check-workflow-uses\.sh(?:\s|$)})
        required_test_callers.each_key do |caller|
          required_test_callers[caller] = true if value.match?(%r{(?:^|[\s/])#{Regexp.escape(caller)}(?:\s|$)})
        end
      end
      walk.call(value, file)
    end
  when Array
    node.each { |value| walk.call(value, file) }
  end
end
Dir.glob(File.join(dir, "*.{yml,yaml}")).sort.each do |file|
  begin
    document = YAML.safe_load(File.read(file), permitted_classes: [], permitted_symbols: [], aliases: false, filename: file)
  rescue Psych::Exception => error
    errors << "#{file}: invalid workflow YAML: #{error.message.lines.first.to_s.strip}"
    next
  end
  walk.call(document, file)
end
errors << "#{dir}: no required workflow invokes ci/check-workflow-uses.sh" unless found_gate
required_test_callers.each do |caller, found|
  errors << "#{dir}: no required CI job invokes #{caller}" unless found
end
ci = File.read(File.join(dir, "ci.yml"))
errors << "#{dir}/ci.yml: pinned-cross must remove legacy apt source files" unless ci.include?("rm -f /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources")
errors << "#{dir}/ci.yml: pinned-cross must upload SHA-named evidence" unless ci.include?('name: pinned-cross-${{ github.sha }}') && ci.include?("target/pinned-cross-evidence")
errors << "#{dir}/ci.yml: pinned-cross evidence upload must retain 90 days" unless ci.match?(/name: pinned-cross-\$\{\{ github\.sha \}\}.*?overwrite: true.*?retention-days: 90/m)
abort(errors.join("\n")) unless errors.empty?
RUBY
