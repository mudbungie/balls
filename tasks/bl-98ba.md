+++
title = "bl install --bin binds the clock_provider (bl-8b98 follow-up: make the documented binding path real)"
created = 1783472890
updated = 1783472890
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
bl-8b98 documented clock_provider as 'bound by bl install --bin', but that path does NOT work: (1) install requires --from or a configured upstream and aborts before binding when neither resolves; (2) bind_referenced only binds names in the [hooks] worklist and REFUSES a --bin name that is not a referenced plugin; (3) resolve_and_bind validates every bind against <bin> protocol, which a clock provider (a bare unix-seconds printer, no protocol subcommand) does not speak. Proven end-to-end: only a hand-made bin/<name> symlink binds a provider today.

FIX (make the documented path real):
- BIND-ONLY install: when no --from is given AND no upstream FETCH_HEAD resolves BUT --bin is provided, skip the config copy and proceed straight to binding. 'bl install --bin <name>=<path>' becomes a pure local re-bind; no config source required.
- The configured clock_provider joins the bind worklist: a --bin name is valid if it is a [hooks]-referenced plugin OR equals config.clock_provider. Plugins keep protocol validation; the clock_provider is validated in its OWN idiom (run it, require one parseable unix-seconds line via crate::clock's probe), then bind bin/<name>. An unbound-but-configured clock_provider reports the same 'referenced but not bound' info line.
- Docs: amend architecture §6 (local binary resolution) + §4/§8 clock_provider so 'bound by bl install --bin' is TRUE.

Makes ~/dev/bl-workhours 'make install' work. 100% coverage, clippy clean, files <=300 lines. bl-chore docs child applies.