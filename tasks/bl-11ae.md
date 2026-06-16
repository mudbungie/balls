+++
title = "Decompose config cluster: conf.rs (246) + config.rs (243)"
created = 1781589016
updated = 1781589016
parent = "bl-fac7"
+++
conf.rs: lift Key+parse+hook_key (37-85) to conf/key.rs AND the resolver cluster (Resolved/resolve/task_remote/origin/scalar/hook_layer/hooks_mentions, 87-242) to conf/resolve.rs → conf.rs ~90. config.rs: lift TOML merge primitives (read_layer/layer_over/ListOp/list_directive/compose_list/as_array, 167-239) to config/merge.rs (shared with hooks, general utility) → config.rs ~166.