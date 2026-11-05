# Upstream test suite

`uniq_test.sh` is the original uniq test suite, vendored verbatim from Apple
[text_cmds-199](https://github.com/apple-oss-distributions/text_cmds/blob/592aaf8a50aa5810ee8183df20f0ba48bb23aa7e/uniq/tests/uniq_test.sh)
(`uniq/tests/uniq_test.sh`, shared with FreeBSD; Copyright (c) 2024 Klara,
Inc., BSD-2-Clause, see the file header). Do not edit it; update it by copying
a newer upstream version.

It is an [ATF](https://github.com/freebsd/atf) shell suite. macOS has no
`atf-sh`/`kyua`, so `atf_shim.sh` implements the subset it uses (`atf_check`
with `-o`/`-e`/`-s`, `atf_set`, `atf_test_case`, `atf_add_test_case`).
`tests/upstream.rs` runs every test case in its own temporary directory and
process group with a `uniq` on `PATH` that is either runiq (through a symlink
named `uniq`, so messages say `uniq:`) or, on macOS, `/usr/bin/uniq` itself,
which validates the shim against the original.
