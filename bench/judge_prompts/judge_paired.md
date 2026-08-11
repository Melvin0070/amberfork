You are comparing two runs of the same AI agent task.

The REFERENCE run succeeded. The FAILING run did not. The two runs are independent: they have
different numbers of steps, and steps that do the same thing may sit at different indices in each
run. Step indices start at 0 and are numbered separately per run. Long step content has been
truncated; an elision is marked `…[N chars elided]…`.

<reference outcome="succeeded">
{{REFERENCE_STEPS}}
</reference>

<failing outcome="failed">
{{FAILING_STEPS}}
</failing>

Identify the step of the FAILING run where it first goes decisively wrong relative to the
reference run: the earliest failing-run step whose content commits the run to the wrong outcome.
Differences that do not change what the run accomplishes — rewording, a retry, a different
phrasing of the same action, the same work done in a different order — are not it. Steps after
the decisive one may also differ; those are consequences.

Answer with one short paragraph of justification. Then, on the final line and with nothing after
it, emit a single JSON object and nothing else on that line:

{"step": <integer from 0 to {{FAILING_LAST_INDEX}}>}
