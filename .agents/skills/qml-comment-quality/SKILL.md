---
name: qml-comment-quality
description: Review QML and embedded JavaScript comments and component documentation using the classics' decision framework and official Qt guidance. Use for comment reviews, diffs, reusable component APIs, QDoc, and qmllint findings. Return evidence-based KEEP, REWRITE, or DELETE verdicts, preserving useful explanations of bindings, signal flows, lifecycle constraints, and non-obvious method chains without changing product logic.
---

# Comment Quality in QML

## What I do

Review comments and return actionable findings: KEEP, REWRITE, DELETE.
Edit comments only when requested. Do not change UI behavior, component APIs,
imports, bindings, handlers, object structure, or lint policy.

Suggest clearer names, typed properties, helpers, or component extraction
separately when they would materially improve clarity.

Propose exact wording only when supported by evidence. Teach how to decide,
not what to copy. Preserve useful knowledge without inventing rationale,
contracts, or guarantees.

## Where this framework comes from

Use Clean Code, Code Complete, Pragmatic Programmer, Fowler, and Kernighan
and Pike as complementary judgment frameworks.

Use official Qt guidance for QML conventions, reactive semantics,
Qt Quick behavior, QDoc, and tooling.

Prefer expressive code, explanations that reduce meaningful reading effort,
and component documentation that consumers can use without inspecting the
internal object tree.

Treat conventions as guidance unless the repository requires them.
An inaccurate comment can be worse than none: readers may treat it as truth.

## Evidence and scope

Read the commented declaration and relevant bindings, handlers, callers,
models, and backend contracts.

Identify the project's Qt version, intended component API, documentation
generator, and available checks when they affect the review.

Stay within the requested scope. Distinguish .qml implementation files from
UI form files, generated code, and standalone JavaScript resources.
Respect their tooling restrictions and required markers.

Separate verified facts from assumptions:

- Missing context does not establish redundancy or obsolescence.
- Retain important unresolved claims provisionally and identify what
  must be checked.
- If implementation and documentation disagree, report the mismatch.
  Do not silently rewrite an intended contract to match a possible bug.
- Never invent UX decisions, platform bugs, signal ordering, ownership,
  performance improvements, or backend guarantees.

## Decision test (in order, for each comment)

1. **Can clearer code remove the explanatory need?**

   Consider better names, explicit property references, typed interfaces,
   named calculations, helpers, or component boundaries.

   Suggest the smallest useful change. Do not perform refactors during a
   comments-only task, and preserve knowledge that would otherwise be lost
   until the proposed change exists.

   A useful comment does not itself prove that the code needs refactoring.
   Component summaries, consumer contracts, and lifecycle requirements
   are not automatically replaced by clearer names.

2. **Does it contribute knowledge or a useful level of explanation?**

   Look for intent, rationale, contracts, warnings, reactive relationships,
   lifecycle constraints, UI requirements, or a semantic overview of a
   non-obvious calculation, signal flow, or method chain.

   For component documentation, also accept a concise behavioral summary.
   Consumers should not need to inspect private children.

   Distinguish behavior readily evident nearby from behavior requiring
   substantial reconstruction across properties, handlers, types, or
   method contracts.

   Prefer an explanation of the overall operation to a translation of
   every declaration. Retain precise details when correctness requires them.

3. **Otherwise: delete it.**

   Delete demonstrably redundant, decorative, or obsolete prose.

   If a comment explains tangled logic, flag the design issue separately.
   Retain useful explanations until the code actually becomes clearer.

   Do not remove essential rationale merely because it is verbose.
   Shorten it only when the relevant facts can be preserved.

Sanity check: try describing the component's responsibility or operation in
one sentence. Difficulty may suggest a design problem; it does not prove one.

## Objectivity rule

Rewriting is not a consolation prize. Elegant narration of obvious
properties and handlers is still noise.

For ordinary implementation notes, DELETE is the default once redundancy
is established. Do not apply that default blindly to component documentation,
external warnings, useful semantic overviews, or unresolved claims.

Information need not be impossible to deduce to be useful.
Being reconstructable is not the same as being evident.

Give each existing logical comment block exactly one verdict:
DELETE, REWRITE, or KEEP.

Every KEEP or REWRITE must name its specific value in one clause:
rationale, component contract, API orientation, semantic overview,
reactive constraint, lifecycle requirement, UX requirement, warning,
or actionable tracking information.

A REWRITE must preserve that value and fix a concrete defect.
Style preference alone is insufficient.

For mixed blocks, retain useful facts and remove noise.
Report missing documentation separately; do not invent an existing comment
or introduce a fourth verdict.

## Bindings, expressions, and method chains

Do not assume declarative syntax or fluent JavaScript is self-explanatory:

    a().b().c(...).d()

Some expressions communicate their purpose immediately. Others require
readers to resolve several transformations, intermediate values, fallback
rules, or interactions between handlers.

Keep or add a concise explanation when it clarifies:

- The overall transformation and the meaning of the result.
- Important intermediate stages or type conversions.
- Why operation order matters.
- How empty values, null, undefined, and failures are handled.
- Which dependencies cause a value to be recalculated.
- Relevant effects, mutation, or interactions with component state.

Prefer one comment above an expression. Use stage comments only when
distinct stages need explanation. Do not narrate every filter(), map(),
or property assignment.

Inspect the actual APIs. JavaScript Array operations are not Rust iterator
adapters: do not describe an Array pipeline as lazy or assume every fluent
API has the same evaluation model.

A QML binding may reevaluate its expression as dependencies change.
Do not describe a reactive calculation as a one-time initialization unless
the implementation actually establishes that behavior.

Explain verified behavior without inventing its rationale.
“Discards empty entries” may be evident from the pipeline; the business
reason for discarding them requires additional evidence.

Keep a helpful chain intact. Suggest named intermediate values or a helper
only when they materially improve readability.

Chain length alone is not a criterion. A familiar long chain may need no
comment; a short but subtle expression may need one.

## When to comment

- **Reactive decisions.** Explain intentional binding replacement, feedback
  prevention, fallback selection, or relationships not clear locally.
- **Lifecycle and dynamic objects.** Explain availability requirements,
  creation-context lifetime, cleanup, or why work waits for a specific event.
- **Signals and state transitions.** Explain the meaning of an event,
  required state, rejected actions, or non-obvious transition behavior.
- **Component boundaries.** Explain dependencies, aliases, model roles,
  backend assumptions, and ownership that consumers need to understand.
- **Geometry and interaction.** Explain meaningful sizing, layout, focus,
  event propagation, accessibility, or animation requirements.
  Do not label every visual child or describe obvious colors and anchors.
- **Algorithms and chains.** Provide a useful semantic overview of complex
  expressions, transformations, or handlers.
- **External constraints.** Record verified Qt, platform, renderer, or native
  backend limitations. Link authoritative evidence when available.
- **Traceable temporaries.** Follow the repository's TODO/FIXME convention.
  State the unresolved work and add a real issue or removal condition when
  available. Never fabricate a reference. Age alone does not justify deletion.

Rule of thumb: keep an implementation comment when removal loses the
reason, constraint, or useful overview needed to maintain the component.
For component documentation, ask whether removal makes correct use harder.

## QML claims to verify

Apply these checks when relevant to the comment under review.
Do not turn this skill into an unsolicited implementation audit.

- **Binding replacement.** An imperative JavaScript assignment can remove
  an existing binding. Distinguish an intentional snapshot from an ongoing
  relationship. Qt.binding() establishes a runtime binding.
- **Evaluation and completion order.** Do not infer binding evaluation order
  from source order. Ordering among Component.onCompleted handlers is
  undefined. Completion is not a general guarantee that asynchronous data
  or final layout/rendering is ready.
- **Deferred work.** Qt.callLater() defers work until the engine returns to
  the event loop and may coalesce calls to the same function. It is not a
  universal layout, frame, or asynchronous-readiness barrier.
- **Property meaning.** required specifies an initialization obligation,
  not domain validation. A readonly binding can still yield changing values;
  readonly does not make a referenced object deeply immutable.
- **Change notification.** Do not assume that mutating a field of a plain
  JavaScript object held in a var property notifies dependent bindings.
  Inspect the actual model, QObject properties, and notification path.
  Do not generalize this limitation to all native objects.
- **Dynamic availability.** Check Loader readiness, item lifetime, and
  creation context before claiming an object is permanently available.
- **Delegate state.** A delegate instance is not necessarily a permanent
  identity for a model row. With ListView reuse enabled, non-model state
  may require reset when the delegate is reused.
- **Geometry ownership.** Layout-managed child geometry can conflict with
  anchors or explicit geometry assignments. A comment does not resolve
  that conflict; report a relevant behavioral mismatch separately.

Do not invent fixes in comments for binding loops, missing notifications,
races, or invalid ownership.

Distinguish a documentation defect from an implementation defect.
Explain the evidence for each.

## Document component behavior, not declarations

Document the intended reusable surface: component purpose, exposed
properties and aliases, signals, methods, and required dependencies.

Do not require documentation for every id or private child object.

- **Properties.** Explain semantic meaning, relevant units, valid ranges,
  defaults, empty/null behavior, and whether the value is supplied by the
  consumer or derived by the component. Include only relevant details.
- **Aliases.** Explain the meaningful capability being exposed and relevant
  write-through or ownership implications. Do not merely repeat the target.
- **Signals.** Explain when emission occurs and what payloads mean.
  Distinguish a requested operation from a completed operation.
  Emitting saveRequested alone does not establish that storage succeeded.
- **Methods.** Explain inputs, return meaning, effects, relevant failure
  behavior, and synchronous versus deferred completion when verified.
- **Visual contract.** Explain sizing expectations, focus behavior, or
  accessibility obligations when consumers must account for them.

A useful summary is valid even when the declaration is simple.
A property named timeout still needs its unit and the meaning of zero
when those affect correct use.

Avoid unsupported guarantees such as “never blocks”, “no leaks”,
“updates automatically”, or “always emits exactly once”.

Put shared component contracts at the component boundary.
Local comments should explain specific implementation decisions rather
than copying that contract throughout the object tree.

## What to delete

Delete literal narration of obvious operations; empty placeholders;
unnecessary decoration; commented-out code; diaries and change histories
better represented in git; and claims confirmed to be obsolete.

Prefer whitespace and normal component organization to repetitive
“Properties”, “Signals”, or “Children” banners, unless a heading provides
useful navigation in unusually large or generated code.

Do not reorder declarations solely to remove headings during a
comments-only task.

Keep useful private explanations, subtle reactive constraints, and concise
references to canonical contracts.

Preserve licenses, required notices, generated-file markers, lint directives,
and design-tool annotations. They are outside ordinary prose cleanup.

## How to write it in QML

Use // for concise implementation notes and /* ... */ when a block comment
fits the repository's style.

Place the explanation next to the declaration, expression, handler, or
lifecycle operation it explains. Keep it precise; use enough detail to
preserve the relevant reasoning.

Use the existing documentation pipeline. Do not require QDoc in a repository
that does not use it, and do not assume Rust's /// or //! conventions
generate QML API documentation.

When QDoc is configured:

- Use /*! ... */ documentation blocks and associate them with the correct
  declaration or explicitly documented topic.
- Use appropriate QML commands such as \qmltype, \inqmlmodule,
  \qmlproperty, \qmlsignal, and \qmlmethod where the context requires them.
  Verify the module, type, signature, and placement against the actual API.
- Follow QDoc markup and project style: \brief for summaries, \c for code,
  \l for links, \a for parameters, and \qml ... \endqml for examples.
- Required type/signature metadata is not redundant prose.
  Explain parameter and return semantics without merely translating
  the declaration.
- Do not impose rustdoc headings, intra-doc link syntax, or doctest rules.

Give reusable components meaningful usage coverage, with required properties,
imports, and dependencies shown or explained.

A shared example can cover related members. Clearly distinguish runnable
examples from partial illustrations.

QDoc rendering does not prove that a snippet runs or behaves correctly.

## One example (deliberately just one)

```qml
import QtQml

QtObject {
    id: root

    property string rawNumbers: ""

    // Keep non-negative safe integers in input order, including duplicates.
    // Discard malformed tokens and values outside JavaScript's safe integer range.
    readonly property string normalizedNumbers: root.rawNumbers
        .split(",")
        .map(token => token.trim())
        .filter(token => /^[0-9]+$/.test(token))
        .map(Number)
        .filter(Number.isSafeInteger)
        .join(", ")
}
```

KEEP — semantic overview and input-handling behavior.

The comment summarizes what survives the entire pipeline without requiring
readers to reconstruct each transformation.

It explains the implemented policy without inventing why the application
chose that policy. “Split, map, filter, and join” would merely narrate syntax.

## Verification and tooling

Use the project's Qt version, module imports, generated type information,
backend registration, and existing build configuration.

Missing tooling metadata can prevent analysis. It does not establish that
a comment is wrong or that the application fails at runtime.

When appropriate and available:

- Run the existing qmllint command or module lint target.
  In CMake projects using qt_add_qml_module(), all_qmllint may be available;
  verify that the target exists.
- Run QDoc through the existing documentation configuration when QDoc
  documentation changed.
- Use relevant existing Qt Quick Test tests or run an example when its
  documented behavior needs checking.
  Do not create a test suite merely to verify a prose-only edit.

qmllint is not a comment-quality evaluator or a proof of signal ordering,
ownership, or runtime behavior.

QML has no universal equivalent of Rust's missing_docs requirement or
automatically executed rustdoc examples.

Preserve directives such as:

    // qmllint disable <category>
    // qmllint enable <category>

They affect analysis, not just prose. If a suppression needs review, check
its actual category, reason, and scope.

Do not remove directives as decoration or add blanket suppressions to
silence findings. Do not run qmllint --fix, broad formatting, or import
cleanup during a comments-only task.

Report which checks ran, unavailable tools, and unrelated failures.
Never claim documentation, rendering, runtime, or test verification beyond
the evidence obtained.

## Procedure

1. Establish scope, audience, Qt version, repository conventions, and evidence.
2. Apply the decision test to each logical comment block.
   Distinguish ordinary comments, component API documentation, QDoc,
   and tooling directives.
3. Assign KEEP, REWRITE, or DELETE with an evidence-based reason.
   Name the specific value of every KEEP or REWRITE.
   Use provisional KEEP for important unresolved claims and state the gap.
   Preserve operational directives outside prose cleanup.
4. Check relevant reactive, lifecycle, signal, model, and geometry claims.
   Review chains for semantic clarity rather than assuming fluent syntax
   makes their behavior evident.
5. Check the reusable component contract and meaningful example coverage.
   Report missing documentation separately from existing-comment verdicts.
6. Propose changes in review mode. Edit comments only when requested.
   Keep refactors, UI behavior changes, and implementation defects separate.
7. Run relevant available documentation or QML checks.
8. Deliver a numbered list containing:
   - File and line, or symbol when line numbers are unavailable.
   - Verdict and specific reason.
   - Severity for proposed changes: blocking for material contract/lifecycle
     defects or required failing checks; suggestion for noise, readability,
     or optional improvements.
   - Exact replacement text for a supported REWRITE; otherwise identify
     the evidence needed.

List KEEP findings individually for a full audit. Otherwise summarize them
so the review remains actionable.

## Official references

Consult the relevant Qt documentation when semantics, generator commands,
or version details need verification.

Follow the project's Qt version instead of blindly applying the latest docs.
Do not browse mechanically for every comment.

- QML Coding Conventions:
  https://doc.qt.io/qt-6/qml-codingconventions.html
- Property Binding:
  https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
- QML Object Attributes:
  https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html
- Signal and Handler Event System:
  https://doc.qt.io/qt-6/qtqml-syntax-signals.html
- Component lifecycle:
  https://doc.qt.io/qt-6/qml-qtqml-component.html
- Dynamic Object Creation:
  https://doc.qt.io/qt-6/qtqml-javascript-dynamicobjectcreation.html
- JavaScript object properties and notifications:
  https://doc.qt.io/qt-6/qml-var.html
- Loader:
  https://doc.qt.io/qt-6/qml-qtquick-loader.html
- ListView and delegate reuse:
  https://doc.qt.io/qt-6/qml-qtquick-listview.html
- Qt Quick Layouts:
  https://doc.qt.io/qt-6/qtquicklayouts-overview.html
- Qt methods, including callLater():
  https://doc.qt.io/qt-6/qml-qtqml-qt.html
- QDoc QML Documentation Style:
  https://doc.qt.io/qt-6/qtwritingstyle-qml.html
- Writing QDoc Documentation:
  https://doc.qt.io/qt-6/qdoc-guide-writing.html
- qmllint:
  https://doc.qt.io/qt-6/qtqml-tooling-qmllint.html
- Qt Quick Test:
  https://doc.qt.io/qt-6/qtquicktest-index.html
