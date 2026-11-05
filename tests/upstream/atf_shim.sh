# Minimal ATF (atf-sh) shim for running uniq_test.sh without kyua.
# Implements: atf_test_case, atf_set, atf_add_test_case, atf_init_test_cases
# (no-ops or registration), and atf_check with -s exit:N, -o/-e
# empty|ignore|inline:TEXT|file:PATH|match:REGEX. Defaults follow atf-sh:
# exit 0, empty stdout, empty stderr.

atf_test_case() { :; }
atf_set() { :; }
atf_add_test_case() { :; }

atf_fail() {
	echo "FAIL: $*" >&2
	exit 1
}

# _atf_expect KIND SPEC FILE
_atf_expect() {
	case "$2" in
	empty) [ ! -s "$3" ] || { echo "--- $1:"; cat "$3"; atf_fail "$1 not empty"; } ;;
	ignore) ;;
	inline:*)
		printf '%b' "${2#inline:}" >"$3.want"
		cmp -s "$3" "$3.want" || { diff -u "$3.want" "$3"; atf_fail "$1 differs"; } ;;
	file:*)
		cmp -s "$3" "${2#file:}" || { diff -u "${2#file:}" "$3"; atf_fail "$1 differs"; } ;;
	match:*)
		grep -Eq -- "${2#match:}" "$3" || { cat "$3"; atf_fail "$1 does not match ${2#match:}"; } ;;
	*) atf_fail "unsupported $1 spec: $2" ;;
	esac
}

atf_check() {
	_status=exit:0 _out=empty _err=empty
	while [ $# -gt 0 ]; do
		case "$1" in
		-s) _status=$2; shift 2 ;;
		-o) _out=$2; shift 2 ;;
		-e) _err=$2; shift 2 ;;
		*) break ;;
		esac
	done
	"$@" >.atf_out 2>.atf_err
	_code=$?
	[ "$_status" = "exit:$_code" ] || { cat .atf_err >&2; atf_fail "$* exited $_code, wanted $_status"; }
	_atf_expect stdout "$_out" .atf_out
	_atf_expect stderr "$_err" .atf_err
	rm -f .atf_out .atf_err .atf_out.want .atf_err.want
}
