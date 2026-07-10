#!/usr/bin/env sh
# Rescue marks (P2) — paint per-tab status glyphs on the sidebar so a restore
# operator watches convergence in place instead of polling `ps`.
#
# Contract:  zellij pipe --name rescue-mark -- '<tab-name>:<state>'
#   state (case-insensitive, aliased):
#     ok   | done    | ready            -> ✅  converged / done
#     wip  | pending | run | running    -> ⏳  in progress
#     fail | err     | error            -> ❌  failed
#     clear| none                       -> remove the mark  ( '*' clears all )
#
# The tab name is matched against the LIVE tab set (a mark for an unknown tab is
# ignored), the payload is split on the LAST ':' (names may contain colons), and
# every mark self-expires after 15 min — so a crashed rescue script cannot
# strand flair. Run this from *inside* the target zellij session.
set -eu

mark() { zellij pipe --name rescue-mark -- "$1:$2"; }

# Worked example: drive three rescue tabs from wip -> ok/fail.
mark '🔨 Rescue' wip
mark '💳 Buy'    wip
mark '📦 Ship'   wip

# ... restore work happens here; flip each tab as it converges ...
sleep 2 && mark '🔨 Rescue' ok
sleep 2 && mark '💳 Buy'    fail
sleep 2 && mark '📦 Ship'   ok

# Clear everything once the operator has eyeballed the outcome:
#   mark '*' clear
