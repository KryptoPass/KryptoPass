---
name: commit-quality
description: Review, plan, and write Git commits using Conventional Commits 1.0.0 and logical change boundaries. Use for commit messages, splitting changes, staged-diff reviews, commitlint findings, and authorized commit creation. Classify by intent, assess compatibility separately, and keep messages supported by the actual diff.
---

# Commit Quality

## What I do

Review commit boundaries and write accurate messages.

Use Conventional Commits for structure and one coherent change per commit
for history quality. Evaluate content, purpose, compatibility, and wording
as separate concerns.

For message or planning requests, return proposals without modifying Git.
Create commits when the user has authorized committing; do not request the
same authorization again.

Follow existing authorization. Do not infer pushing, publishing releases,
or rewriting shared history from drafting a message or creating a local
commit alone.

Do not change product code to make a proposed message true.

## Sources and rule precedence

Use:

- Conventional Commits 1.0.0 for message structure.
- Git contributor guidance for logical changes and useful explanations.
- Semantic Versioning for compatibility concepts when the project uses it.
- The repository's configuration for enforceable types, scopes, formatting,
  trailers, hooks, and release behavior.

Follow, in order:

1. The user's instructions.
2. Applicable repository contribution and workflow rules.
3. Active commitlint and release configuration.
4. This skill's defaults.

Distinguish standard requirements, repository requirements, and recommended
style. Do not present personal preferences as specification violations.

Treat Git-project or Linux-kernel conventions as examples, not universal
requirements for signoffs, prefixes, or patch submission.

If the repository uses another established convention, preserve it and
apply this skill's reasoning without mechanically converting its history.

## Evidence first

Read the requested diff, not only filenames, a task description, or the
last conversation message.

Inspect relevant context, callers, and contracts when needed to understand
what changed and who is affected.

Read contribution rules, active commitlint/release configuration, and enough
recent history to identify established scopes and language. Historical
examples do not override explicit configuration.

Distinguish:

- Changes already committed.
- Changes staged for the next commit.
- Unstaged working-tree changes.
- New or untracked files.

For an ordinary staged commit, its message must describe what will actually
be recorded. Inspect new files before including them.

When only a description is available, provide a qualified proposal.
Do not claim that a diff, staged snapshot, or repository configuration
was inspected.

Never invent issue numbers, rationale, migration steps, test results,
benchmarks, or compatibility guarantees.

Missing context does not justify choosing refactor, chore, or non-breaking
by default.

## Decision test (in order)

1. **What is the concrete purpose?**

   Identify the bug, capability, maintenance task, or structural change.
   Describe the resulting behavior or structure in one sentence.

   If the purpose is unclear, identify the missing evidence. Do not hide
   uncertainty behind a generic message.

2. **Is this one coherent change?**

   Group implementation, tests, documentation, and required generated or
   lockfile changes that support the same purpose.

   Separate unrelated fixes, features, refactors, formatting, or dependency
   updates.

   Do not split merely by file, language, extension, or commit type.
   A supporting refactor may belong with a feature when separation would
   produce an artificial or unusable intermediate change.

3. **Is the committed snapshot coherent?**

   Order dependencies so each retained commit is meaningful and buildable
   under the relevant project configuration.

   A commit may depend on earlier commits. Atomic does not mean independent
   of its entire history.

   Avoid introducing an unusable intermediate snapshot merely to satisfy
   a mechanical splitting rule.

4. **Does compatibility change?**

   Assess supported interfaces, configuration, behavior, data formats,
   and platforms independently of the chosen type.

5. **Does the message match the change?**

   Choose type and scope from actual intent, then describe the result.
   Add a body only when it preserves useful context.

Commit size and file count are signals, not hard thresholds.
A rename across many files can be one coherent change.
Two changed lines can contain unrelated fixes.

## Conventional Commits format

    <type>[optional scope][optional !]: <description>

    [optional body]

    [optional footer(s)]

Use a type, colon, space, and non-empty description.
Put an optional scope in parentheses.
Put ! immediately before the colon for a breaking change.

Separate the header from the body, and the footer section from preceding
content, with blank lines when those sections are present.

Conventional Commits defines feat and fix semantics, permits additional
types, and allows a breaking marker on any type.

The taxonomy below is a fallback policy, not a claim that the specification
mandates every listed type.

## Choose type by intent

| Type | Default meaning |
| --- | --- |
| feat | Introduce a capability. |
| fix | Correct incorrect behavior. |
| refactor | Restructure code without adding a capability or intentionally changing runtime behavior; assess interface compatibility separately. |
| perf | Improve performance, with evidence supporting the claimed effect. |
| docs | Change documentation or explanatory comments without changing runtime behavior. |
| style | Change formatting or syntax presentation without changing behavior. |
| test | Add or correct tests without an accompanying product change. |
| build | Change build tooling, packaging, or dependencies, following repository policy. |
| ci | Change continuous integration configuration or automation. |
| chore | Perform maintenance that does not fit a more informative permitted type. |
| revert | Revert an earlier change, following the repository's established format. |

Choose the type for the commit's main purpose.

A bug fix with its regression test is normally one fix commit.
A feature with its usage documentation is normally one feat commit.
Do not separate supporting changes merely to give each its own type.

Do not use chore as a generic label for changes you have not understood.

Dependency updates may be build(deps), chore(deps), or fix when they
demonstrably correct a bug. Follow actual intent and repository taxonomy.

### Rust and QML distinctions

- Comment-only changes in .rs or .qml can be docs.
  The extension does not determine the type.
- A new internal helper does not automatically introduce a feature.
  It may support a refactor or an existing behavior.
- QML colors, spacing, layout, and interactions are product/UI changes,
  not automatically style. Classify the intended correction or capability.
- Renaming a supported Rust method or exposed QML property may require
  a breaking marker even when its purpose is structural.
- Introducing a new public name does not by itself establish a new feature.
  Inspect whether it adds capability or replaces an existing interface.
- Shorter code does not establish a performance improvement.
  Report measured results only when measured. A verified reduction in work
  can be described without inventing a percentage.

Type and compatibility are separate decisions.
refactor does not guarantee compatibility, and fix does not rule out
an incompatible change.

## Choose a useful scope

Use an established subsystem, package, or component name when it helps
readers understand where the change belongs.

Respect configured scope lists. Reuse existing meaningful names rather
than inventing a new scope for every file.

Do not combine unrelated scopes to disguise mixed work.
Do not use a long path as a substitute for understanding the change.

Omit the scope when it adds no useful information and policy permits it.

## Write the subject

Use a concrete verb and meaningful object.
State what changes after applying the commit.

Prefer imperative phrasing and omit a trailing period by default.
Preserve the spelling and case of identifiers.

Avoid vague subjects such as:

- update stuff
- miscellaneous fixes
- improve code
- changes
- WIP

Describe the result rather than listing files or saying that work occurred.

Follow repository language and casing. Use lower-case types by default.

Follow configured length limits. Without one, aim for a complete header
of about 72 characters and readable body lines around 72 characters.

These are style defaults, not Conventional Commits requirements.
Preserve clarity, URLs, and exact identifiers instead of mechanically
breaking them.

Do not reject a useful message solely because it differs from an optional
stylistic preference.

## Write the body and footers

Use a body when it preserves information useful to a future reviewer:

- The trigger or previous behavior.
- The reason for the change.
- The resulting behavior.
- A relevant trade-off.
- Compatibility or migration implications.

Do not narrate the diff line by line, repeat the subject, or record the
coding session.

Include implementation mechanics when they explain the decision.
Avoid boilerplate sections for trivial changes.

Use genuine issue references. Use a closing reference only when the change
resolves the issue and the repository's integration supports that syntax.

Do not invent Signed-off-by, Reviewed-by, Co-authored-by, or other
attestations.

Include validation when repository policy or the change makes it useful.
Report only checks actually run.

The presence of tests in a diff does not establish that they passed.
Avoid embedding transient local test logs unnecessarily.

## Breaking changes

Assess incompatibility against the supported contract, not the type label,
line count, or perceived size of the change.

Check, when relevant:

- Removed or renamed public members.
- Incompatible parameters, return values, or behavioral guarantees.
- Changed configuration names or required inputs.
- Incompatible persisted or exchanged data formats.
- Removed supported platforms or toolchain versions.

Use ! in the header or a BREAKING CHANGE: footer as the specification allows.
Follow stricter repository rules when present.

For a substantial incompatible change, prefer both markers when policy
permits, with concrete affected consumers and migration guidance.
This is a recommendation of this skill, not a universal requirement.

When using the footer, retain uppercase BREAKING CHANGE and explain
the incompatibility. “Breaking changes” alone is insufficient.

Do not claim a removal when a compatible alias remains.
Do not mark purely internal renaming as breaking without an affected
supported interface.

Do not predict a release number or automatically bump versions from a
commit prefix. Check release configuration, package mapping, and pre-1.0
policy before making versioning claims.

Formatting a commit does not execute a release.

## One example

Assume the diff confirms the delegate reset and its purpose:

```text
fix(vault): reset copied state when reusing delegates

Reused delegates retained the copied indicator from their previous row.
Reset the local flag on reuse so another credential does not inherit
that indicator.
```

The subject identifies the correction.
The body preserves the symptom and why the reset matters.

Keep the implementation, regression test, and related component
documentation together when they support this same correction.

Do not replace this with “chore(ui): improve interface”.
Do not claim the regression test passed unless it was run successfully.

## Staging and execution

For authorized commit creation, inspect status and full relevant diffs.
Useful commands include:

    git status --short
    git diff
    git diff --cached
    git diff --cached --check

Use explicit files or selected hunks when needed.

Broad staging is appropriate only when the entire set was reviewed and
belongs to the authorized scope.

Preserve existing user changes and staging intent.
Do not include unrelated pre-staged work or silently unstage it to force
the proposed plan.

Before committing, inspect the final staged diff and align the message
with it. Run required and appropriate checks.

When relevant unstaged changes affect behavior, passing tests on the
working tree does not establish that the staged snapshot passed.
Validate that snapshot or state the limitation.

Respect existing hooks and signing requirements.
Do not bypass hooks merely to obtain a successful commit.

If hooks modify files, inspect the changes before selectively restaging
and retrying.

Use a message file for multiline CLI messages.

After creation, inspect the resulting commit and remaining status.
Confirm its actual contents and report what remains uncommitted.

Do not amend, rebase, or force-push merely to polish history.
Those operations must be covered by the user's authorized scope.

## Review and output

For an existing or proposed commit, identify whether it:

- Is ready.
- Needs rewording.
- Needs splitting.
- Cannot be finalized because material scope, content, or compatibility
  evidence is unresolved.

Explain the actual defect. Do not treat optional wording preferences
as blockers.

For proposals, deliver:

- The ordered commit plan.
- Exact messages.
- Which files or hunks belong to each commit, when known.
- Dependencies between commits.
- Material unverified assumptions.

Do not claim staging or creation occurred when only proposing messages.

For execution, report commit hashes and subjects, relevant validation,
and remaining changes.

Keep the response proportionate to the work.
A simple, coherent change may need only one subject line.

## Procedure

1. Establish requested mode, existing authorization, repository rules,
   and available evidence.
2. Inspect actual changes and distinguish staged from unstaged content.
3. Identify logical purposes and keep supporting changes together.
4. Order coherent commits and assess supported compatibility separately.
5. Select type and useful scope from intent and repository policy.
6. Write the subject, useful body, and genuine footers.
   Check every claim against the proposed or committed contents.
7. Run existing applicable lint and validation checks when appropriate.
   Do not install commitlint, hooks, or release automation unless requested.
8. Return proposals or create authorized commits.
   Verify and report the actual result without exaggerating evidence.

## Official references

Consult the relevant specification or tool documentation when a rule
or configuration detail needs verification.

- Conventional Commits 1.0.0:
  https://www.conventionalcommits.org/en/v1.0.0/
- Git — contribution and commit guidelines:
  https://git-scm.com/book/en/v2/Distributed-Git-Contributing-to-a-Project
- Git — creating commits:
  https://git-scm.com/docs/git-commit
- Git — staging changes:
  https://git-scm.com/docs/git-add
- Git — inspecting differences:
  https://git-scm.com/docs/git-diff
- commitlint — configurable rules:
  https://commitlint.js.org/reference/rules.html
- Semantic Versioning:
  https://semver.org/
