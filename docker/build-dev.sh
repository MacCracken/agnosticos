#!/usr/bin/env bash
# docker/build-dev.sh — build the AGNOS container stack: a clean layered family on
# the mirshi model. Each tier is the one below it + exactly one delta layer.
#
#   agnos-thin   FROM scratch      : /mirshi + iam (identity) + ark (package mgr) — the base
#   agnos-shell  FROM agnos-thin   : + agnsh (the shell) — a usable env
#   agnos-dev    FROM agnos-shell  : + kriya/owl/kii/bnrmr/cmdrs — the full dev env
#
# NO Linux base, NO GNU shell, NO QEMU — agnos userland runs natively under mirshi
# (cf. mirshi/docker/build.sh). The exec band (execwait#37 / spawn_path#43 /
# exec_redirect#62 / symlink#63) needs mirshi >= 1.10.2.
#
# Usage:
#   docker/build-dev.sh                   # build the whole stack: thin -> shell -> dev
#   PROFILE=thin  docker/build-dev.sh     # just the base
#   PROFILE=shell docker/build-dev.sh     # base + shell
#   TAG_PREFIX=ghcr.io/you/ REPOS=/path docker/build-dev.sh
set -uo pipefail

target="${PROFILE:-dev}"
repos="${REPOS:-$HOME/Repos}"
mirshi="$repos/mirshi"
here="$(cd "$(dirname "$0")" && pwd)"
pfx="${TAG_PREFIX:-}"

case "$target" in thin|shell|dev) : ;; *) echo "error: PROFILE must be thin|shell|dev" >&2; exit 1 ;; esac
command -v cyrius >/dev/null || { echo "error: cyrius not on PATH" >&2; exit 1; }
command -v docker >/dev/null || { echo "error: docker not on PATH" >&2; exit 1; }
[ -d "$mirshi" ] || { echo "error: mirshi repo not found at $mirshi (set REPOS=)" >&2; exit 1; }

# master tool registry: output -> "repo entry"
declare -A TOOL=(
  [iam]="iam src/main.cyr"          [ark]="ark src/main.cyr"
  [agnsh]="agnoshi src/agnsh.cyr"   [kriya]="kriya src/main.cyr"
  [owl]="owl src/main.cyr"          [kii]="kii src/main.cyr"
  [bnrmr]="bannermanor src/main.cyr" [cmdrs]="commandress src/main.cyr"
  [cyim]="cyim src/main.cyr"         [sit]="sit src/main.cyr"
  [shu]="chakshu src/main.cyr"       [hapi]="hapi src/main.cyr"
  [yo]="yo src/main.cyr"             [dig]="dig src/main.cyr"
  [whirl]="whirl src/main.cyr"       [darshini]="darshini src/main.cyr"
  [anuenue]="anuenue src/main.cyr"
  [agora]="agora src/main.cyr"       [descent]="cyrius-yeomans-descent src/main.cyr"
)
# the stack (base -> up): delta tools · Dockerfile · keystone tool (must build; "-" = none)
tiers=(thin shell dev)
declare -A DELTA=( [thin]="iam ark" [shell]="agnsh" [dev]="kriya owl kii bnrmr cmdrs cyim sit shu hapi yo dig whirl darshini anuenue agora descent" )
declare -A DFILE=( [thin]=Dockerfile.thin [shell]=Dockerfile.shell [dev]=Dockerfile.dev )
declare -A KEY=(   [thin]=ark [shell]=agnsh [dev]="-" )

bins="$(mktemp -d)"; trap 'rm -rf "$bins"' EXIT
declare -A HAVE=()

build_tool() {  # $1=output; compiles --agnos into $bins/<out> once (cached). returns 0 on success.
  local out="$1"; [ -n "${HAVE[$out]:-}" ] && return 0
  read -r repo entry <<<"${TOOL[$out]}"
  [ -d "$repos/$repo" ] || { echo "    ✗ $out — repo $repos/$repo missing"; return 1; }
  if ( cd "$repos/$repo" && cyrius build --agnos "$entry" "$bins/$out" ) >/dev/null 2>&1; then
    HAVE[$out]=1; echo "    ✓ $out ($repo)"; return 0
  fi
  echo "    ✗ $out ($repo) — cyrius build --agnos failed"; return 1
}

echo "==> building mirshi (Linux-target supervisor) from $mirshi"
( cd "$mirshi" && cyrius build src/main.cyr "$bins/mirshi" ) >/dev/null || { echo "error: mirshi build failed" >&2; exit 1; }

parent=""
for tier in "${tiers[@]}"; do
  tag="${pfx}agnos-${tier}"
  echo "==> [$tier] $tag  (${DELTA[$tier]})"
  ok=(); for out in ${DELTA[$tier]}; do build_tool "$out" && ok+=("$out"); done
  if [ "${KEY[$tier]}" != "-" ]; then
    printf '%s\n' "${ok[@]:-}" | grep -qx "${KEY[$tier]}" \
      || { echo "error: keystone '${KEY[$tier]}' did not build --agnos ($tier)" >&2; exit 1; }
  fi
  # assemble this tier's build context: only its delta binaries (+ mirshi/data for the base)
  stage="$(mktemp -d)"; mkdir -p "$stage/bin"
  for out in "${ok[@]:-}"; do [ -n "$out" ] && cp "$bins/$out" "$stage/bin/$out"; done
  cp "$here/${DFILE[$tier]}" "$stage/Dockerfile"
  args=()
  if [ "$tier" = thin ]; then
    cp "$bins/mirshi" "$stage/mirshi"; mkdir -p "$stage/data"
    echo "AGNOS — agnos userland under mirshi (no VM)." > "$stage/data/motd.txt"
  else
    args=(--build-arg "PARENT=$parent")
  fi
  docker build "${args[@]}" -t "$tag" "$stage" >/dev/null || { echo "error: docker build failed ($tier)" >&2; rm -rf "$stage"; exit 1; }
  rm -rf "$stage"
  docker image inspect "$tag" --format "    $tag — {{.Size}} bytes, {{len .RootFS.Layers}} layers" 2>/dev/null || true
  parent="$tag"
  [ "$tier" = "$target" ] && break
done

echo ""
echo "    built the stack up to: ${pfx}agnos-${target}"
echo "    run:  docker run --rm -it --cap-add=SYS_PTRACE --security-opt seccomp=unconfined ${pfx}agnos-${target}"
echo "          (stock Docker's seccomp blocks mirshi's ptrace, hence the caps)"
exit 0
