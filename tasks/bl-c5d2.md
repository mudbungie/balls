+++
title = "Flaky test: a_bare_id_reaches_the_prefixed_ball_on_every_id_argument picks tasks/.gitkeep as 'the other ball' by directory order — CI red on main since 569f041c"
created = 1795156794
updated = 1795156794
claimant = "Without"
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-082c"
on = "close"
+++
bl-bb1d's dispatch test finds the dependent ball as the first tasks/ entry that is not the first ball's file. tasks/ also holds .gitkeep, so by filesystem order the 'dependent' can be .gitkeep and the content assert fails (CI run 44673669594; passed on the builder and two earlier CI runs by luck). Fix: only .md entries are balls.