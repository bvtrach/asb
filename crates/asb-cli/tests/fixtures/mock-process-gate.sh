#!/bin/sh
[ "$1" = __asb-gate ] || exit 11
[ "$2" = --asb-launch-v1 ] || exit 12
[ "$3" = 0 ] || exit 13
[ "$4" = fixture-client ] || exit 14
[ "$5" = 'argument with spaces' ] || exit 15
[ "$6" = 'literal;$value' ] || exit 16
[ "$ASB_GATE_FIXTURE" = 'explicit-value' ] || exit 17
[ -z "${HOME+x}" ] || exit 18
printf 'fixture gate accepted\n'
