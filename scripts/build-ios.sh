#!/usr/bin/env bash
# iOS packaging commands must fail closed when their declared deployment floor
# drifts: a binary built for a newer iOS version cannot run on the supported
# iOS 15 floor.
#
# Scope, stated plainly: this validates the DECLARED floor only. It reads the
# real Xcode build setting (IPHONEOS_DEPLOYMENT_TARGET) out of the project and
# asserts CI agrees with it. It does NOT inspect a Mach-O, because no iOS
# binary is built yet — there are no sources and no targets. The macOS
# equivalent is stricter on purpose: scripts/build-macos.sh --verify-bundle
# reads `minos` from the actual binary with vtool/otool. Once an iOS target
# produces a binary, this gate must be upgraded the same way.
set -euo pipefail

readonly approved_ios_deployment_floor='15.0'

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly project_file_default="$repo_root/apps/ios/VitelaIOS.xcodeproj/project.pbxproj"
readonly ci_file_default="$repo_root/.github/workflows/ios.yml"

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
Usage:
  scripts/build-ios.sh --validate-deployment-floor

The validation requires IOS_DEPLOYMENT_FLOOR=15.0 and checks that the Xcode
project's IPHONEOS_DEPLOYMENT_TARGET and the Apple CI workflow declare that
same approved deployment floor.
EOF
}

declared_floor() {
  local file=$1
  local pattern=$2
  local source_name=$3
  local value

  [[ -f "$file" ]] || die "missing $source_name deployment-floor source: $file"
  value=$(sed -nE "$pattern" "$file")
  [[ -n "$value" ]] || die "missing iOS deployment floor in $source_name: $file"
  [[ $(printf '%s\n' "$value" | wc -l) -eq 1 ]] || die "ambiguous iOS deployment floor in $source_name: $file"
  printf '%s\n' "$value"
}

validate_deployment_floor() {
  local environment_floor=${IOS_DEPLOYMENT_FLOOR:-}
  local project_file=${IOS_PROJECT_FILE:-$project_file_default}
  local ci_file=${IOS_CI_FILE:-$ci_file_default}
  local project_floor ci_floor

  [[ -n "$environment_floor" ]] || die 'IOS_DEPLOYMENT_FLOOR is required'
  [[ "$environment_floor" == "$approved_ios_deployment_floor" ]] || die "IOS_DEPLOYMENT_FLOOR must be exactly $approved_ios_deployment_floor; got '$environment_floor'"

  project_floor=$(declared_floor "$project_file" 's/^[[:space:]]*IPHONEOS_DEPLOYMENT_TARGET[[:space:]]*=[[:space:]]*([0-9]+(\.[0-9]+)?);[[:space:]]*$/\1/p' 'Xcode project')
  [[ "$project_floor" == "$approved_ios_deployment_floor" ]] || die "iOS deployment floor mismatch: project declares $project_floor, expected $approved_ios_deployment_floor"

  ci_floor=$(declared_floor "$ci_file" 's/^[[:space:]]*IOS_DEPLOYMENT_FLOOR:[[:space:]]*"?([0-9]+(\.[0-9]+)?)"?[[:space:]]*$/\1/p' 'Apple CI workflow')
  [[ "$ci_floor" == "$approved_ios_deployment_floor" ]] || die "iOS deployment floor mismatch: CI declares $ci_floor, expected $approved_ios_deployment_floor"

  printf 'ok: iOS deployment floor is %s in environment, project, and CI\n' "$approved_ios_deployment_floor"
}

case ${1:-} in
  --validate-deployment-floor)
    validate_deployment_floor
    ;;
  --help|-h|'')
    usage
    ;;
  *)
    die "unknown command: $1"
    ;;
esac
