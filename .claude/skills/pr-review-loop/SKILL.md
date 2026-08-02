---
name: "pr-review-loop"
description: "Work a CodeRabbit review on a pull request to completion: collect findings, disposition each one, amend fixes into the commit that introduced them, and answer every thread. Use when asked to address, respond to, or iterate on PR review comments."
argument-hint: "Optional PR number (defaults to the PR for the current branch)"
compatibility: "Requires gh with pull-request write access, and jq"
user-invocable: true
disable-model-invocation: false
---

# PR review loop

The constitution requires every CodeRabbit finding to be **fixed or answered in its thread** before
a human reviews, and requires the fix to be a proper part of the change rather than a "review fixes"
commit tacked on the end. That combination is what makes this more than "read comments, edit files".

## 1. Collect

Findings and replies come back from the same endpoint. Separate them — `in_reply_to_id` is null on a
finding, set on a reply — and keep only the bot's, since this loop answers a CodeRabbit review:

```bash
gh api repos/{owner}/{repo}/pulls/N/comments --paginate --slurp > tmp/c.json
jq -r 'add // [] | .[]
       | select(.in_reply_to_id == null and .user.login == "coderabbitai[bot]")
       | "\(.id)\t\(.path):\(.line // .original_line)"' tmp/c.json
```

A root comment from a human reviewer is deliberately not collected here. It is not a CodeRabbit
finding, it is not what the pre-human review gate is about, and sweeping it into the same list
invites answering a colleague with a disposition label. Read those separately and reply as yourself.

**Always fetch to a file, then filter it.** `gh api … | jq …` reports jq's exit status, not `gh`'s,
so an API failure arrives as an empty result that looks like "nothing to do" — the loudest failure
mode the constitution forbids, wearing the costume of a quiet no-op. Redirect, check, then filter.

**`--slurp` on every paginated fetch, and `add` in every filter that consumes one.** Past one page
`gh api --paginate` emits one JSON array *per page*, which is not a single JSON document; a filter
run over it silently yields one result per page. `--slurp` wraps the pages in an outer array and
`add` concatenates them back into the flat array the filters expect. `add // []` covers the case
where there are no pages at all.

**Also read the review body.** CodeRabbit files some findings there rather than inline — under
"Duplicate comments" or "Additional comments". They are real findings and the summary count at the
top ("Actionable comments posted: N") does not include them:

```bash
gh api repos/{owner}/{repo}/pulls/N/reviews --paginate --slurp > tmp/reviews.json
jq -r 'add // [] | .[] | select(.user.login == "coderabbitai[bot]") | .body' tmp/reviews.json
```

A finding filed in the review body has no thread of its own, so there is nowhere to reply in-thread
and no `id` to reply to. It gets a PR-level comment instead — see §4. Collect these separately from
the inline findings; the two surfaces are answered through different endpoints.

## 2. Disposition each finding

Label every finding **agree**, **partially agree**, or **disagree** before touching a file. Verify
the claim against the current tree first — findings are anchored to a commit, and a force-push may
have already invalidated one.

Ask the user, rather than deciding, when a finding turns on a **product choice**: a public
identifier, a supported-platform matrix, a version floor, anything a reviewer could reasonably
overturn later. Do not ask about wording, structure, or which requirement should carry a rule.

Push back when a finding asserts a **fact it did not verify** — a version number read off a
changelog, a claim about what an external tool supports. Check it against something real (a local
checkout, the actual API) and say what you found. Reviewer findings are evidence, not instructions.

Push back when a finding asks for an artifact **outside the change's scope**, such as an
implementation file in a specification-only PR. Convert it into the requirement that will force the
artifact later, and say so.

## 3. Fix, amended into the commit that introduced the problem

`git rebase -i` is unavailable in this environment. Use a detached checkout and replay instead —
this preserves authorship and touches only the target commit:

```bash
git stash push -m fixes <paths>
git checkout --detach <target-commit>
git stash pop
git add <paths>
git commit --amend -F tmp/msg.txt     # --no-edit only if the message still fits the change
NEW=$(git rev-parse HEAD)
git rebase --onto "$NEW" <target-commit> <branch>
git push --force-with-lease origin <branch>
```

Confirm afterwards that only the intended commit changed: `git diff <old-tip> <new-tip> --stat`
should list exactly the files you edited.

`--no-edit` is the wrong default here. The commit message is part of the change under review, and a
review fix routinely invalidates it — a message that explained a trade-off the fix has since removed
is now wrong, and a fix that introduces a new one leaves the message silently incomplete. Re-read it
against the amended diff, and rewrite it with `-F` unless it still describes what the commit does.
Conventional Commits subject, and a body explaining any non-obvious trade-off; the constitution
requires both, and neither survives on autopilot.

If a local branch also points at the rewritten commit — a spec committed on `main` before the branch
was cut, say — it is now stale and will conflict at merge. Raise it; do not silently move it.

## 4. Answer every finding

An inline finding is answered in its own thread:

```bash
gh api repos/{owner}/{repo}/pulls/N/comments/<finding-id>/replies -F "body=@tmp/reply.md"
```

A review-body finding has no thread, so it is answered in a PR-level comment — a different endpoint,
and the one place where the answer has to carry its own context:

```bash
gh api repos/{owner}/{repo}/issues/N/comments -F "body=@tmp/reply.md"
```

Because that comment floats free of any diff anchor, it must name what it is answering: the review
it came from, the file and line the finding cited, and the finding's own heading. One PR-level
comment may answer several review-body findings as long as each is identified that way. Without
those references the comment is unreviewable — nobody can tell which finding it disposed of.

**`-F`, never `-f`.** With `--raw-field`/`-f` the string `@reply.md` is posted verbatim as the
comment body; GitHub returns 200 and nothing fails. Only `--field`/`-F` substitutes the file's
contents. If it has already happened, every broken body names its own source file, so the repair is
self-describing:

```bash
# Only your own comments, and only paths that stay inside tmp/ -- see below.
jq -r --arg me "$(gh api user --jq .login)" \
   'add // [] | .[]
    | select(.user.login == $me and (.body | test("^@[^\\s]+\\.md\\s*$")))
    | "\(.id)\t\(.body)"' tmp/c.json > tmp/broken.tsv

root=$(realpath tmp)
while IFS=$'\t' read -r id path; do
	file=$(realpath -- "${path#@}") || continue
	case "$file" in "$root"/*) ;; *) printf 'refusing %s\n' "$path" >&2; continue ;; esac
	[ -f "$file" ] || continue
	gh api -X PATCH "repos/{owner}/{repo}/pulls/comments/${id}" -F "body=@$file"
done < tmp/broken.tsv
```

Both guards matter, and neither is paranoia about your own typo. The body being matched is
**attacker-controlled text**: anyone who can comment on the pull request can post `@../../.ssh/notes.md`,
and an unguarded repair loop will read that file off your disk and publish it to a public thread.
`-F` reading local files is exactly the behavior that makes the `-f`/`-F` bug above so easy to miss,
and it is the same behavior that makes this dangerous. Restricting to comments you authored removes
the injection path; resolving with `realpath` and requiring the result to sit under `tmp/` removes
the traversal and symlink cases that survive a plain string check.

Verify after posting — re-fetch and check no body still starts with `@`.

A reply states the disposition, what changed and why, and names the commit the fix landed in (the
original line anchors no longer resolve after a force-push). A disagreement gets its reasoning, not
just a refusal. Partial agreement says which part was taken and which was declined.

## 5. Next round

CodeRabbit reviews again on each force-push. Poll for a review **authored by the bot** — posting
your own replies creates review objects too, so an unfiltered check fires on your own work.

Record the newest bot review *before* pushing, so the poll can tell the next round from the one just
worked:

```bash
# A round is a bot review with a body. Empty-bodied ones are not rounds -- see below.
round_ids() {
	jq --argjson since "$1" \
	   'add // [] | [.[]
	    | select(.id > $since and .user.login == "coderabbitai[bot]" and (.body | length) > 0)
	    | .id] | sort' tmp/reviews.json
}

gh api repos/{owner}/{repo}/pulls/N/reviews --paginate --slurp > tmp/reviews.json || exit 1
LAST=$(round_ids 0 | jq 'max // 0')

# ... force-push, then wait for the round it triggers.
deadline=$((SECONDS + 1800))
until [ "$(round_ids "$LAST" | jq 'length')" -gt 0 ]; do
	[ "$SECONDS" -lt "$deadline" ] || { printf 'no review after 30m\n' >&2; exit 1; }
	sleep 30
	gh api repos/{owner}/{repo}/pulls/N/reviews --paginate --slurp > tmp/reviews.json || exit 1
done

LAST=$(round_ids "$LAST" | jq 'max')   # carry the baseline forward before the next round
```

Three things in that loop are load-bearing, and each one cost a wasted cycle to learn:

**A bot-authored review object is not a review.** With `chat.auto_reply` enabled, every reply
CodeRabbit posts into a thread creates a review object of its own — bot login, valid id, and an
**empty body**. Answering four threads manufactures four of them within seconds, so a poll filtered
only on the login fires immediately and reports a round that does not exist. The body is what
distinguishes a round from a courtesy reply.

**`LAST` has to move.** Detecting the new round is not the same as recording it. Leave `LAST` at the
previous baseline and the next poll matches the round you already worked and returns instantly.

**`max // 0` is what makes the first round work**: before any review exists the array is empty,
`max` is `null`, and every real review id is greater than `0`.

The deadline is there because the alternative is an agent waiting forever on a review that a service
outage means will never arrive. A review that does not come is a failure and must say so.

Expect later rounds to change character: internal contradictions first, then regressions introduced
by the previous round's fixes, then gaps against the constitution. When a round is mostly
regressions from the last one, say so in the summary — it is the signal that fixes are not being
carried through to every place that quotes them.

Between rounds, do not amend and push again unless there is something to fix; a push restarts the
review.

**Three rounds is the ceiling — three rounds of *fixes*.** Rounds one and two each end in a
force-push that triggers the next. Round three is the last one whose findings get code changes:
disposition them, fix, answer, push.

That push triggers one more review, and it is still collected and answered in full — every finding
dispositioned, every thread replied to. What it does not get is code changes, so nothing is pushed
and no further round is triggered; the loop terminates there. Any finding that would require a code
change is recorded as an explicit unresolved state that blocks human review, and named in the
report.

The distinction matters because the cap and the constitution otherwise contradict each other. The
governance rule is that every finding is fixed **or answered** before a human looks at the pull
request, and walking away from a review nobody has read fails it — the cap would have licensed
exactly the silence the rule exists to prevent. Capping the fixing is what was wanted; capping the
answering was never on offer.

The cap is there because the rounds converge, and a fourth still producing substantive findings is
evidence about the change rather than about the review. Either the change is too large to review in
one piece, or the underlying spec is unsettled and the review is discovering that one contradiction
at a time. Splitting the pull request or settling the spec fixes that; another pass does not. Say
which one it looks like instead of continuing.

## 6. Close out

Log anything deferred in `SESSION.local.md` — never in a commit. Report honestly: findings fixed,
findings answered but not applied and why, and anything left open. If a review round is still
pending when work stops, say that rather than implying the loop finished — and if the loop stopped
at the three-round ceiling rather than because the review came back clean, that distinction is the
most important thing in the report.
