+++
title = "Bound the prime fixpoint for real — §8/§13 claim 'bounded by the §6 invocation-tree cap'; no bound exists"
created = 1781036077
updated = 1781038936
claimant = "Redress"
parent = "bl-72a8"
priority = 2
+++
architecture.md §8:576-577: 'It is bounded by the §6 invocation-tree cap and converges in one pass when the dial holds'; §13:1109-1110 repeats the claim. The code cannot deliver it: src/lifecycle_diffless.rs:99-104 is a bare loop { run pre; if step()? { break } } with no iteration counter, and the §6 depth cap (BALLS_PLUGIN_DEPTH) structurally cannot bound it — depth grows DOWN the invocation tree, while this loop iterates ACROSS passes at the same depth+1. A prime.pre plugin that rewrites tasks_branch on every pass loops forever.

Fix: a small loud iteration cap on the convergence loop (fail + name the op and the oscillating dial value), mirroring the depth-cap's fail-not-silent disposition (§6:497-507 'Crossing the cap ABORTS the op — fail, not silent… There is no hatch'). Then correct the spec sentence to name the real bound (§15 entry).

**Why:** default plugins never trigger it (the tracker never rewrites the dial), but this is exactly the class of footgun §6/bl-7110 mandates fail-loud guards for — and the spec currently claims a guarantee the code does not have. Two representations of one fact have drifted; make the code the truth and re-derive the sentence.