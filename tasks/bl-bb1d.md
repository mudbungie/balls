+++
title = "Bare ids, flipped: what is typed wins; only on a miss is bl-<typed> tried (replaces bl-91be's lexical 4-hex rule)"
created = 1794984278
updated = 1794984278
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
User redirect (2026-11-17) of bl-91be: do not rewrite by shape. Look up the id as typed; if that is a miss, try `bl-`+typed; if that misses too, carry on with what was typed (so errors name it).

'Miss' is each consumer's own lookup: the mutating verbs' ball positional must be LIVE; `show` and the edge/parent flags accept live-or-dead (recency). One resolver (id::resolve) parameterised by that predicate. Dissolves bl-91be's accepted hole (a plugin-assigned bare-hex id is exact, so it wins) and drops the shape rule entirely — any typed token gets the fallback.