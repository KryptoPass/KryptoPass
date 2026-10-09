---
name: rust-comment-quality
description: Review Rust implementation comments and rustdoc using the classics' decision framework and official Rust guidance. Use for comment reviews, diffs, public API documentation, and documentation lint findings. Return evidence-based KEEP, REWRITE, or DELETE verdicts, preserving useful explanations of non-obvious method chains without rewriting product logic.
---

# Comment Quality in Rust

## What I do

Review comments and return actionable findings: KEEP, REWRITE, DELETE.
Edit comments only when requested. Do not change product logic, signatures,
visibility, or lint policy.

Suggest a refactor separately when clearer code could remove the need for
an implementation explanation. Propose exact wording only when supported
by the available evidence.

Teach how to decide, not what to copy. Preserve useful knowledge without
inventing rationale, contracts, or guarantees.

## Where this framework comes from

Use the original classics as a judgment framework: Clean Code (ch. 4),
Code Complete (ch. 32), Pragmatic Programmer (DRY), Fowler (comments as
deodorant), and Kernighan and Pike (don't belabor the obvious).

Use the Rust compiler development guide, API Guidelines, rustdoc book,
and rustc and Clippy documentation for Rust-specific practice.

Treat these sources as complementary guidance, not identical or absolute
rules. Compiler-specific conventions do not automatically apply to every
Rust repository.

Favor expressive code, explanations that reduce meaningful reading effort,
and documentation that lets consumers use an API without inspecting its
implementation. An inaccurate comment is worse than none: readers may
treat it as truth.

## Evidence and scope

Read the commented item and enough surrounding code to check its claims.
Inspect relevant types, call sites, tests, dependency contracts, and
repository conventions when they affect the verdict.

Stay within the requested scope. Do not turn a diff review into a
repository-wide cleanup.

Separate verified facts from assumptions:

- Missing context does not prove redundancy or obsolescence.
- When an important claim cannot be verified, retain it provisionally and
  state what needs checking.
- When documentation and implementation disagree, report the mismatch.
  Do not silently rewrite the intended contract to match a possible bug.
- Never invent historical decisions, issue numbers, safety arguments,
  performance claims, or failure guarantees.

## Decision test (in order, for each comment)

1. **Can clearer code remove the explanatory need?**

   Consider a better name, extracted function or variable, named constant,
   assertion, or type that expresses the condition at its use sites.

   Suggest the smallest useful refactor. Do not perform it during a
   comments-only task, and do not delete knowledge that still depends on
   the proposed refactor becoming explicit.

   A useful comment does not itself prove the code needs refactoring.
   Public summaries, caller contracts, and local safety arguments are
   not automatically replaced by better names.

2. **Does it contribute knowledge or a useful level of explanation?**

   For implementation notes, look for intent, rationale, warnings,
   invariants, ordering constraints, external limits, or an overview of
   a non-obvious algorithm or method chain.

    For rustdoc, also accept concise behavioral summaries and caller-facing
    semantics, but only when they state semantics the signature does not
    give (see “Document behavior, not signatures”). A summary that merely
    restates the name and types in prose is paraphrase, no matter how well
    written. API consumers should not need to inspect the implementation.

   Distinguish information readily evident from nearby code from information
   that requires reconstructing several operations, resolving intermediate
   types, or consulting method contracts.

   Prefer an explanation of the overall operation to a line-by-line
   translation. Retain precise low-level facts when safety or correctness
   requires them.

3. **Otherwise: delete it.**

   Delete demonstrably redundant, decorative, or obsolete prose.

   If a comment explains tangled code, flag the design issue separately.
   Preserve the useful explanation until the code actually becomes clearer.

Sanity check: try describing the routine in one short sentence. Difficulty
may suggest a design problem; it is not proof that a refactor is required.

## Objectivity rule

Rewriting is not a consolation prize. Elegant paraphrases of obvious code
are still noise.

For ordinary implementation notes, DELETE is the default once redundancy
is established. Do not apply that default blindly to public documentation,
safety explanations, unresolved warnings, or useful semantic overviews.

Information need not be impossible to deduce to deserve a comment.
Being reconstructable is not the same as being evident.

Give each existing logical comment block exactly one verdict:
DELETE, REWRITE, or KEEP.

Every KEEP or REWRITE must name its specific value in one clause:
rationale, caller contract, API orientation, semantic overview, invariant,
warning, local safety argument, or actionable tracking information.

A category name is not the citation: “API orientation” or “semantic
overview” alone does not justify a verdict. Quote or pinpoint what the
reader learns that the signature does not say; if there is nothing to
quote, the verdict is DELETE.

A REWRITE must preserve that value and fix a concrete defect. Style
preference alone is insufficient. For mixed blocks, retain useful facts
and remove redundant material.

Report missing documentation separately on the item. Do not invent an
existing comment or introduce a fourth verdict.

## Method chains and pipelines

Do not assume that fluent Rust code is self-explanatory:

    a().b().c(...).d()

Some chains communicate their purpose immediately. Others hide important
transformations behind adapters, generic return types, closures, builders,
or short method names.

Keep or add a concise explanation when it helps readers understand:

- The overall transformation and the meaning of the result.
- Important intermediate stages or type transitions.
- Why operation order matters.
- Whether failures are propagated, accumulated, or discarded.
- Whether evaluation is lazy, short-circuiting, or triggered by a
  consuming operation.
- Relevant ownership changes or side effects.

Explain the semantic operation, not every method call. A comment above the
chain is usually enough; use stage comments only when distinct stages
need explanation.

Do not invent a reason for the chosen behavior when only the behavior is
known. “Discards malformed entries” may be verified; “discards malformed
entries because the upstream service is unreliable” requires evidence.

Do not require refactoring merely because a chain benefits from a comment.
Suggest named intermediate values or a helper only when they materially
improve readability.

Conversely, chain length alone does not justify commentary. A familiar,
straightforward chain may need none; a short but subtle chain may need one.
Do not use a fixed method-count threshold.

## When to comment

- **Non-obvious decisions.** Explain the deciding constraint or trade-off.
  Include discarded alternatives only when they clarify the current choice.
- **Invariants.** Explain conditions the type system does not establish and
  where they are maintained. Do not treat debug-only assertions as release
  guarantees.
- **Ordering and concurrency.** Explain initialization sequences,
  synchronization, ownership, or races that adjacent calls do not reveal.
- **External limits.** Record relevant dependency bugs, platform differences,
  or protocol constraints. Link authoritative evidence when available.
- **Algorithms and chains.** Explain the overall operation or important
  stages when reconstructing them imposes meaningful reading effort.
- **Safety.** Document caller obligations and justify unsafe operations
  at their use sites.
- **Public contract.** Explain behavior, effects, errors, guarantees, and
  meaningful costs or limitations.
- **Traceable temporaries.** Follow the repository's TODO/FIXME convention.
  State the unresolved work and add a real issue or removal condition when
  available. Never fabricate a reference. Age alone does not justify deletion.

Rule of thumb: keep an implementation comment when removal loses the
reason, constraint, or useful overview needed to maintain the code.
For rustdoc, ask whether removal makes the API harder to understand or use.

## Document behavior, not signatures

A clear signature does not replace a useful API summary. Document
observable semantics instead of transcribing parameter and return types.

- **Option:** explain what absence means, plus matching rules or effects
  when relevant. “Finds a credential by its identifier” is a useful summary;
  “Takes a string slice and returns an Option” adds no semantics.
- **Result:** describe relevant error conditions and partial progress or
  state guarantees. Verify atomicity, rollback, persistence, and cleanup
  before claiming them. A Result type alone proves none of these.
- **Invariants:** document shared guarantees at the type, trait, or module
  level. Use a # Invariants section when helpful; it is a convention, not a
  required rustdoc heading. Methods still document their own preconditions
  and effects.

Local safety comments may reference a shared invariant, but must explain
why it holds at the operation being justified.

Do not add claims such as “never plaintext”, “not retained”, “constant-time”,
“no side effects”, or “storage unchanged on error” without sufficient evidence.

Document relevant async cancellation, blocking, or resource behavior when
verified and important to consumers.

## What to delete

Delete literal narration of obvious operations; empty placeholders;
unnecessary decoration; commented-out code; diaries and change histories
better represented in git; and claims confirmed to be obsolete.

Remove trivial closing-brace labels unless they serve a demonstrated
readability need.

Do not require documentation on every private item unless repository policy
says so. Keep useful private documentation, non-obvious explanations, and
short references to canonical contracts.

Preserve required notices, licenses, generated-file markers, and tooling
directives. They are outside ordinary prose cleanup.

## How to write it in Rust

- Use /// for the following item, //! for the enclosing crate or module,
  and // for implementation notes. Prefer line comments; do not churn valid
  block comments merely to change style.
- Start rustdoc with a concise behavioral summary, normally a sentence with
  capitalization and a period. Follow the project's voice; third-person
  verbs are common, not a compiler requirement. The first paragraph appears
  in overviews and search results.
- Document behavior, contracts, relevant limitations, and safety. Avoid
  redundant listings of types already shown in the signature.
- Add sections where their content applies:
  - # Examples for meaningful usage.
  - # Errors for relevant error conditions.
  - # Panics for relevant reachable panic conditions.
  - # Safety for unsafe caller or implementer obligations.
  Do not add empty sections or enumerate every conceivable panic.
- Give public APIs meaningful example coverage. Link an applicable example
  on a type, module, or related item instead of duplicating it when sufficient.
  Show why to use the API, not merely how to invoke it.
- Prefer ? in fallible usage examples. Use hidden # setup to keep examples
  complete. Assertions, unwrap(), or expect() can be justified for known
  test data or intentional panic demonstrations; avoid teaching casual
  unwrapping of fallible operations.
- Use runnable doctests by default. Use no_run when code should compile but
  cannot safely or reliably run, and compile_fail for intentionally rejected
  code. Use ignore only with a specific reason.
- Put symbols in backticks. Use name-based intra-doc links such as [`Vec`]
  or [`Self::find`] when helpful. Use Markdown links for external
  specifications, issues, and papers. External URLs are allowed.
- Do not use #[doc(hidden)] to evade documentation work: it affects display,
  not visibility. Treat changes such as pub(crate) as separate API/design work.
- Put shared trait contracts on the trait definition. Document relevant
  implementation-specific behavior where consumers can find it.

## Safety documentation

Use # Safety on unsafe functions for obligations callers must uphold, and
on unsafe traits for obligations implementers must uphold.

State concrete conditions, not “the caller must ensure this is safe”.
A safe wrapper must establish its own safety conditions; prose cannot
transfer undefined-behavior prevention to callers of a safe API.

Require a nearby // SAFETY: explanation for unsafe blocks and unsafe impls
as this review policy.

Explain why all relevant preconditions hold at this site: initialization,
alignment, validity, lifetimes, aliasing, ownership, or synchronization,
as applicable. Explain preservation for the required duration.

A prefix alone is not a safety argument. If the argument fails, report the
potential soundness defect separately. Never repair an unsupported safety
claim by inventing a stronger comment.

## One example (deliberately just one)

```rust
fn parse_ids(lines: &[String]) -> Vec<u32> {
    // Keep successfully parsed identifiers in input order;
    // malformed lines are silently discarded.
    lines
        .iter()
        .map(|line| line.trim().parse::<u32>())
        .filter_map(Result::ok)
        .collect()
}
```

KEEP — semantic overview and error handling behavior.

The behavior can be reconstructed from the methods, but the comment
communicates the overall operation without requiring readers to mentally
execute the chain.

“Iterate, map, filter, and collect” would merely narrate the syntax.

## Enforcing lints

Use the correct lint namespace and the project's supported toolchain:

- rustc: missing_docs.
- rustdoc: rustdoc::missing_crate_level_docs,
  rustdoc::broken_intra_doc_links, rustdoc::invalid_rust_codeblocks,
  rustdoc::bare_urls.
- Clippy: clippy::missing_errors_doc, clippy::missing_panics_doc,
  clippy::missing_safety_doc, clippy::doc_markdown,
  clippy::undocumented_unsafe_blocks.

Respect existing lint levels. missing_errors_doc, missing_panics_doc,
and doc_markdown are pedantic lints; undocumented_unsafe_blocks is a
restriction lint. Do not assume every listed lint is enabled by default,
or enable entire groups without a project-specific reason.

missing_docs concerns public API coverage, not every private item.
Lints check patterns and presence; they do not establish that prose is
true or unsafe code is sound.

invalid_rust_codeblocks is not a substitute for compiling doctests.

When appropriate and available, run:

    cargo doc --no-deps
    cargo test --doc
    <the project's Clippy command>

Match the package, target, feature, and MSRV configuration. cargo test --doc
targets library documentation; do not assume it verifies every documentation
source in the repository.

Use --document-private-items for internal documentation reviews when needed.
Do not assume --all-features is valid for every project.

Report unavailable checks and unrelated existing failures separately.
Never claim verification beyond the checks actually performed.

## Procedure

1. Establish scope, audience, repository conventions, and available evidence.
2. Apply the decision test to each logical comment block. Classify ///, //!,
   or //; flag a wrong kind when it affects purpose or rendering.
3. Assign KEEP, REWRITE, or DELETE with an evidence-based reason. Name the
   specific value of every KEEP or REWRITE. For unresolved important claims,
   use provisional KEEP and state the evidence gap.
4. Review method chains for semantic clarity, rather than assuming idiomatic
   syntax makes their behavior evident.
5. Check public summaries, meaningful example coverage, applicable contracts,
   and local safety explanations. Report missing documentation separately.
6. In review mode, propose changes. In requested edit mode, change comments
   only. Keep refactors and behavioral defects separate.
7. Run relevant available documentation checks.
8. Deliver a numbered list containing:
   - File and line, or symbol when line numbers are unavailable.
   - Verdict and specific reason.
   - Severity for proposed changes: blocking for material contract/safety
     defects or required failing checks; suggestion for noise, readability,
     or optional improvements.
   - Exact replacement wording for a supported REWRITE; otherwise state
     what evidence is needed.

Include KEEP findings individually for a full audit. Otherwise summarize
them so the review remains actionable.

## Official references

Consult the relevant source when a rule, lint, or toolchain detail needs
verification; do not browse mechanically for every comment.

- Rust compiler conventions:
  https://rustc-dev-guide.rust-lang.org/conventions.html
- Rust API Guidelines — documentation:
  https://rust-lang.github.io/api-guidelines/documentation.html
- Rustdoc — writing documentation:
  https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html
- Rustdoc — documentation tests:
  https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html
- Rustdoc — intra-doc links:
  https://doc.rust-lang.org/rustdoc/write-documentation/linking-to-items-by-name.html
- Rustdoc — lints:
  https://doc.rust-lang.org/rustdoc/lints.html
- Clippy lint catalog:
  https://rust-lang.github.io/rust-clippy/stable/index.html
- Rust Reference — unsafe:
  https://doc.rust-lang.org/reference/unsafe-keyword.html
- Rust By Example — iterating over Results:
  https://doc.rust-lang.org/rust-by-example/error/iter_result.html
