use super::{Document, MAX_SOURCE_BYTES, RenderedDocument};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn action_at(rendered: &RenderedDocument, index: usize) -> Result<String, Box<dyn Error>> {
    rendered
        .actions
        .get(index)
        .cloned()
        .ok_or_else(|| format!("Missing spoiler action {index}; got {:?}", rendered.actions).into())
}

fn assert_absent(rendered: &RenderedDocument, private_parts: &[&str]) {
    for private in private_parts {
        assert!(
            !rendered.html.contains(private),
            "Private or forbidden content {private:?} leaked into HTML: {}",
            rendered.html.as_str()
        );
        for action in rendered.actions.iter().chain(&rendered.revealed_actions) {
            assert!(
                !action.contains(private),
                "Private content {private:?} leaked into action {action:?}"
            );
        }
    }
}

// Check the emitted subset, including attributes and links, without depending on
// a complete HTML snapshot or the global revision counter's value.
fn assert_allowed_html(rendered: &RenderedDocument) -> TestResult {
    const TAGS: &[&str] = &[
        "p",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "blockquote",
        "ul",
        "ol",
        "li",
        "hr",
        "br",
        "em",
        "strong",
        "s",
        "pre",
        "code",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "a",
        "span",
    ];

    for fragment in rendered.html.split('<').skip(1) {
        let (tag, _) = fragment.split_once('>').ok_or("Unterminated HTML tag")?;
        let tag = tag.trim().trim_end_matches('/').trim_end();
        let (closing, tag) = match tag.strip_prefix('/') {
            Some(tag) => (true, tag),
            None => (false, tag),
        };
        let (name, mut attributes) = match tag.split_once(char::is_whitespace) {
            Some(parts) => parts,
            None => (tag, ""),
        };
        assert!(TAGS.contains(&name), "Forbidden HTML tag: <{tag}>");
        if closing {
            assert!(attributes.is_empty(), "Attributes on closing tag: {tag}");
        }

        while !attributes.trim_start().is_empty() {
            let (attribute, rest) = attributes
                .trim_start()
                .split_once('=')
                .ok_or("HTML attribute without a value")?;
            let rest = rest
                .strip_prefix('"')
                .ok_or("HTML attribute without double quotes")?;
            let (value, rest) = rest.split_once('"').ok_or("Unterminated HTML attribute")?;
            let allowed: &[&str] = match name {
                "a" => &["href", "style"],
                "span" | "pre" => &["style"],
                "table" => &["border", "cellspacing", "cellpadding", "style"],
                "th" | "td" => &["align"],
                "ol" => &["start"],
                _ => &[],
            };
            assert!(
                allowed.contains(&attribute),
                "Forbidden attribute {attribute:?} on <{name}>"
            );
            match attribute {
                "href" => {
                    let url = value.to_ascii_lowercase();
                    assert!(
                        rendered.actions.iter().any(|action| action == value)
                            || ["https:", "http:", "mailto:"]
                                .iter()
                                .any(|scheme| url.starts_with(scheme)),
                        "Forbidden href: {value:?}"
                    );
                }
                "style" => {
                    for declaration in value.split(';').filter(|part| !part.is_empty()) {
                        let (property, value) = declaration
                            .split_once(':')
                            .ok_or("CSS declaration without a value")?;
                        let properties: &[&str] = match name {
                            "a" => &["color"],
                            "span" => &["background-color", "color"],
                            "pre" => &["background-color"],
                            "table" => &["border-color", "border-collapse"],
                            _ => &[],
                        };
                        assert!(
                            properties.contains(&property),
                            "Forbidden CSS property {property:?} on <{name}>"
                        );
                        if property == "border-collapse" {
                            assert_eq!(value, "collapse");
                        } else {
                            assert!(
                                value.len() == 7
                                    && value.starts_with('#')
                                    && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit()),
                                "Unsafe CSS color: {value:?}"
                            );
                        }
                    }
                }
                "align" => assert!(matches!(value, "left" | "center" | "right")),
                "border" | "cellspacing" | "cellpadding" | "start" => {
                    value.parse::<u64>()?;
                }
                _ => return Err(format!("Unchecked HTML attribute {attribute:?}").into()),
            }
            attributes = rest;
        }
    }

    for action in &rendered.actions {
        let href = format!("href=\"{action}\"");
        assert_eq!(
            rendered.html.matches(&href).count(),
            1,
            "Each exported action must have exactly one visible link: {action}; HTML: {}",
            rendered.html.as_str()
        );
    }
    for action in &rendered.revealed_actions {
        assert!(rendered.actions.contains(action));
    }
    Ok(())
}

#[test]
fn default_and_empty_documents_have_no_content_or_actions() -> TestResult {
    let mut document = Document::default();
    for _ in 0..2 {
        let rendered = document.render()?;
        assert!(rendered.html.is_empty());
        assert!(rendered.actions.is_empty());
        assert!(rendered.revealed_actions.is_empty());
        document.load("")?;
    }
    assert!(!document.toggle("kryptopass-spoiler:0:0"));
    document.hide_all();
    assert!(document.render()?.html.is_empty());
    Ok(())
}

#[test]
fn hidden_spoiler_omits_text_url_code_and_action_metadata() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "PUBLIC_BEFORE ||PRIVATE_TEXT **PRIVATE_STRONG** ",
        "[PRIVATE_LABEL](https://private.example.test/PRIVATE_URL) ",
        "`PRIVATE_CODE <tag> & value`|| PUBLIC_AFTER"
    ))?;

    let rendered = document.render()?;
    assert!(rendered.html.contains("PUBLIC_BEFORE"));
    assert!(rendered.html.contains("PUBLIC_AFTER"));
    assert_eq!(rendered.actions.len(), 1);
    assert!(rendered.revealed_actions.is_empty());
    assert_absent(
        &rendered,
        &[
            "PRIVATE_TEXT",
            "PRIVATE_STRONG",
            "PRIVATE_LABEL",
            "private.example.test",
            "PRIVATE_URL",
            "PRIVATE_CODE",
            "<strong>",
            "<code>",
        ],
    );
    assert_allowed_html(&rendered)
}

#[test]
fn reveal_preserves_strong_code_and_link_and_hide_removes_them() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "PUBLIC ||**SECRET_STRONG** `SECRET_CODE <tag> & value` ",
        "[SECRET_LINK](https://secret.example.test/path?a=1&b=2)||"
    ))?;
    let action = action_at(&document.render()?, 0)?;

    assert!(document.toggle(&action));
    let revealed = document.render()?;
    assert_eq!(revealed.revealed_actions, vec![action.clone()]);
    assert!(revealed.html.contains("<strong>SECRET_STRONG</strong>"));
    assert!(
        revealed
            .html
            .contains("<code>SECRET_CODE &lt;tag&gt; &amp; value</code>")
    );
    assert!(
        revealed
            .html
            .contains("href=\"https://secret.example.test/path?a=1&amp;b=2\"")
    );
    assert!(revealed.html.contains("SECRET_LINK</a>"));
    assert_allowed_html(&revealed)?;

    assert!(document.toggle(&action));
    let hidden = document.render()?;
    assert_eq!(hidden.actions, vec![action]);
    assert!(hidden.revealed_actions.is_empty());
    assert!(hidden.html.contains("PUBLIC"));
    assert_absent(
        &hidden,
        &[
            "SECRET_STRONG",
            "SECRET_CODE",
            "SECRET_LINK",
            "secret.example.test",
        ],
    );
    assert_allowed_html(&hidden)
}

#[test]
fn two_spoilers_toggle_independently() -> TestResult {
    let mut document = Document::default();
    document.load("||FIRST_SECRET|| public ||SECOND_SECRET||")?;
    let hidden = document.render()?;
    assert_eq!(hidden.actions.len(), 2);
    let first = action_at(&hidden, 0)?;
    let second = action_at(&hidden, 1)?;
    assert_ne!(first, second);
    assert_absent(&hidden, &["FIRST_SECRET", "SECOND_SECRET"]);

    assert!(document.toggle(&first));
    let first_only = document.render()?;
    assert!(first_only.html.contains("FIRST_SECRET"));
    assert_absent(&first_only, &["SECOND_SECRET"]);
    assert_eq!(first_only.revealed_actions, vec![first.clone()]);

    assert!(document.toggle(&second));
    let both = document.render()?;
    assert!(both.html.contains("FIRST_SECRET"));
    assert!(both.html.contains("SECOND_SECRET"));
    assert_eq!(both.revealed_actions, vec![first.clone(), second.clone()]);

    assert!(document.toggle(&first));
    let second_only = document.render()?;
    assert_absent(&second_only, &["FIRST_SECRET"]);
    assert!(second_only.html.contains("SECOND_SECRET"));
    assert_eq!(second_only.actions, hidden.actions);
    assert_eq!(second_only.revealed_actions, vec![second]);
    assert_allowed_html(&second_only)
}

#[test]
fn hide_all_hides_every_spoiler_and_keeps_current_actions_usable() -> TestResult {
    let mut document = Document::default();
    document.load("||FIRST_SECRET|| and ||SECOND_SECRET||")?;
    let hidden = document.render()?;
    assert_eq!(hidden.actions.len(), 2);
    for action in &hidden.actions {
        assert!(document.toggle(action));
    }
    assert_eq!(document.render()?.revealed_actions.len(), 2);

    document.hide_all();
    document.hide_all();
    let reset = document.render()?;
    assert_eq!(reset.actions, hidden.actions);
    assert!(reset.revealed_actions.is_empty());
    assert_absent(&reset, &["FIRST_SECRET", "SECOND_SECRET"]);
    assert!(document.toggle(&action_at(&reset, 1)?));
    let second_only = document.render()?;
    assert!(second_only.html.contains("SECOND_SECRET"));
    assert_absent(&second_only, &["FIRST_SECRET"]);
    Ok(())
}

#[test]
fn repeated_render_preserves_revealed_state_and_actions() -> TestResult {
    let mut document = Document::default();
    document.load("||SECRET_STABLE||")?;
    let initial = document.render()?;
    let hidden_again = document.render()?;
    assert_eq!(initial.html, hidden_again.html);
    assert_eq!(initial.actions, hidden_again.actions);
    let action = action_at(&initial, 0)?;
    assert!(document.toggle(&action));

    let revealed = document.render()?;
    let revealed_again = document.render()?;
    assert!(revealed_again.html.contains("SECRET_STABLE"));
    assert_eq!(revealed.html, revealed_again.html);
    assert_eq!(revealed.actions, revealed_again.actions);
    assert_eq!(revealed_again.revealed_actions, vec![action]);
    Ok(())
}

#[test]
fn identical_load_hides_spoilers_and_invalidates_previous_actions() -> TestResult {
    let source = "||SECRET_RELOADED||";
    let mut document = Document::default();
    document.load(source)?;
    let old_action = action_at(&document.render()?, 0)?;
    assert!(document.toggle(&old_action));
    assert!(document.render()?.html.contains("SECRET_RELOADED"));

    document.load(source)?;
    assert!(!document.toggle(&old_action));
    let reset = document.render()?;
    assert_absent(&reset, &["SECRET_RELOADED"]);
    assert!(reset.revealed_actions.is_empty());
    let new_action = action_at(&reset, 0)?;
    assert_ne!(old_action, new_action);
    assert!(!document.toggle(&old_action));
    assert!(document.toggle(&new_action));
    assert!(document.render()?.html.contains("SECRET_RELOADED"));
    Ok(())
}

#[test]
fn clear_removes_content_and_invalidates_actions_even_after_reload() -> TestResult {
    let mut document = Document::default();
    document.load("PUBLIC_OLD ||SECRET_OLD||")?;
    let old_action = action_at(&document.render()?, 0)?;
    assert!(document.toggle(&old_action));
    assert!(document.render()?.html.contains("SECRET_OLD"));

    document.clear();
    assert!(!document.toggle(&old_action));
    let cleared = document.render()?;
    assert!(cleared.html.is_empty());
    assert!(cleared.actions.is_empty());
    assert!(cleared.revealed_actions.is_empty());

    document.load("||SECRET_NEW||")?;
    let current = document.render()?;
    assert_absent(&current, &["PUBLIC_OLD", "SECRET_OLD", "SECRET_NEW"]);
    assert!(!document.toggle(&old_action));
    assert!(document.toggle(&action_at(&current, 0)?));
    assert!(document.render()?.html.contains("SECRET_NEW"));
    Ok(())
}

#[test]
fn document_actions_and_revealed_states_are_independent() -> TestResult {
    let mut first = Document::default();
    let mut second = Document::default();
    first.load("||SAME_SECRET||")?;
    second.load("||SAME_SECRET||")?;
    let first_action = action_at(&first.render()?, 0)?;
    let second_action = action_at(&second.render()?, 0)?;
    assert_ne!(first_action, second_action);
    assert!(!first.toggle(&second_action));
    assert!(!second.toggle(&first_action));

    assert!(first.toggle(&first_action));
    assert!(first.render()?.html.contains("SAME_SECRET"));
    let second_hidden = second.render()?;
    assert_absent(&second_hidden, &["SAME_SECRET"]);
    assert!(second_hidden.revealed_actions.is_empty());

    assert!(second.toggle(&second_action));
    first.clear();
    let second_revealed = second.render()?;
    assert!(second_revealed.html.contains("SAME_SECRET"));
    assert_eq!(
        second_revealed.revealed_actions,
        vec![second_action.clone()]
    );
    assert!(second.toggle(&second_action));
    assert_absent(&second.render()?, &["SAME_SECRET"]);
    Ok(())
}

#[test]
fn malformed_and_unavailable_actions_do_not_change_state() -> TestResult {
    let mut document = Document::default();
    document.load("||SECRET_ACTION||")?;
    let hidden = document.render()?;
    let action = action_at(&hidden, 0)?;
    let (prefix, _) = action.rsplit_once(':').ok_or("Action missing index")?;
    let unavailable = format!("{prefix}:{}", usize::MAX);
    for invalid in [
        "",
        "https://example.test/",
        "kryptopass-spoiler:",
        "kryptopass-spoiler:1",
        "kryptopass-spoiler:not-a-revision:0",
        "kryptopass-spoiler:1:not-an-index",
        "kryptopass-spoiler:1:0:extra",
        "kryptopass-spoiler:18446744073709551616:0",
        unavailable.as_str(),
    ] {
        assert!(
            !document.toggle(invalid),
            "Accepted invalid action {invalid:?}"
        );
    }
    let unchanged = document.render()?;
    assert_eq!(unchanged.html, hidden.html);
    assert_eq!(unchanged.actions, hidden.actions);
    assert!(unchanged.revealed_actions.is_empty());
    assert!(document.toggle(&action));
    assert!(document.render()?.html.contains("SECRET_ACTION"));
    Ok(())
}

#[test]
fn spoiler_inside_link_omits_outer_url_and_never_nests_anchors() -> TestResult {
    let mut document = Document::default();
    document.load(
        "[PUBLIC_LABEL ||**SECRET_LABEL**||](https://outer.example.test/PRIVATE_DESTINATION)",
    )?;
    let hidden = document.render()?;
    assert!(hidden.html.contains("PUBLIC_LABEL"));
    assert_absent(
        &hidden,
        &["SECRET_LABEL", "outer.example.test", "PRIVATE_DESTINATION"],
    );
    assert_eq!(hidden.html.matches("<a ").count(), 1);
    assert_eq!(hidden.html.matches("</a>").count(), 1);
    let action = action_at(&hidden, 0)?;

    assert!(document.toggle(&action));
    let revealed = document.render()?;
    assert!(revealed.html.contains("<strong>SECRET_LABEL</strong>"));
    assert_absent(&revealed, &["outer.example.test", "PRIVATE_DESTINATION"]);
    assert_eq!(revealed.html.matches("<a ").count(), 1);
    assert_eq!(revealed.html.matches("</a>").count(), 1);
    assert_allowed_html(&revealed)?;

    assert!(document.toggle(&action));
    assert_absent(&document.render()?, &["SECRET_LABEL", "outer.example.test"]);
    Ok(())
}

#[test]
fn spoiler_inside_image_never_leaks_alt_url_title_or_actions() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "![ALT_PUBLIC ||ALT_SECRET||](https://image.example.test/IMAGE_URL \"IMAGE_TITLE\") ",
        "||VISIBLE_SECRET||"
    ))?;
    let hidden = document.render()?;
    assert!(hidden.html.contains("[Imagen omitida]"));
    assert_eq!(hidden.actions.len(), 1);
    assert_absent(
        &hidden,
        &[
            "ALT_PUBLIC",
            "ALT_SECRET",
            "image.example.test",
            "IMAGE_URL",
            "IMAGE_TITLE",
            "VISIBLE_SECRET",
            "<img",
        ],
    );
    let visible_action = action_at(&hidden, 0)?;
    let (prefix, _) = visible_action
        .rsplit_once(':')
        .ok_or("Action missing index")?;
    assert!(!document.toggle(&format!("{prefix}:0")));
    assert!(document.toggle(&visible_action));
    let revealed = document.render()?;
    assert!(revealed.html.contains("VISIBLE_SECRET"));
    assert_absent(
        &revealed,
        &["ALT_SECRET", "image.example.test", "IMAGE_TITLE", "<img"],
    );
    assert_allowed_html(&revealed)?;

    document.load("![||ONLY_IMAGE_SECRET||](file:///ONLY_IMAGE_URL)")?;
    let image_only = document.render()?;
    assert_eq!(image_only.html.trim(), "<p>[Imagen omitida]</p>");
    assert!(image_only.actions.is_empty());
    assert!(image_only.revealed_actions.is_empty());
    assert_absent(&image_only, &["ONLY_IMAGE_SECRET", "ONLY_IMAGE_URL"]);
    Ok(())
}

#[test]
fn images_and_reference_images_omit_every_resource_scheme() -> TestResult {
    for url in [
        "https://image.example.test/RESOURCE_SECRET.png",
        "http://image.example.test/RESOURCE_SECRET.png",
        "file:///RESOURCE_SECRET.png",
        "data:image/png;base64,RESOURCE_SECRET",
    ] {
        for source in [
            format!("PUBLIC ![ALT_SECRET]({url} \"TITLE_SECRET\")"),
            format!("PUBLIC ![ALT_SECRET][image]\n\n[image]: {url} \"TITLE_SECRET\""),
        ] {
            let mut document = Document::default();
            document.load(&source)?;
            let rendered = document.render()?;
            assert!(rendered.html.contains("PUBLIC"));
            assert!(rendered.html.contains("[Imagen omitida]"));
            assert!(rendered.actions.is_empty());
            assert_absent(
                &rendered,
                &[
                    "ALT_SECRET",
                    "TITLE_SECRET",
                    "RESOURCE_SECRET",
                    "<img",
                    "src=",
                ],
            );
            assert_allowed_html(&rendered)?;
        }
    }
    Ok(())
}

#[test]
fn raw_html_blocks_and_resource_tags_are_omitted() -> TestResult {
    for raw in [
        "<script>RAW_SECRET</script>",
        "<style>body { background: url(https://resource.example.test/RAW_SECRET); }</style>",
        "<iframe src=\"https://resource.example.test/RAW_SECRET\">RAW_SECRET</iframe>",
        "<object data=\"file:///RAW_SECRET\">\nRAW_SECRET\n</object>",
        "<img src=\"https://resource.example.test/RAW_SECRET\" onerror=\"RAW_SECRET()\">",
        "<link rel=\"stylesheet\" href=\"https://resource.example.test/RAW_SECRET\">",
        "<!-- RAW_SECRET -->",
    ] {
        let source = format!("PUBLIC_BEFORE\n\n{raw}\n\nPUBLIC_AFTER");
        let mut document = Document::default();
        document.load(&source)?;
        let rendered = document.render()?;
        assert!(rendered.html.contains("PUBLIC_BEFORE"));
        assert!(rendered.html.contains("PUBLIC_AFTER"));
        assert_absent(
            &rendered,
            &[
                "RAW_SECRET",
                "resource.example.test",
                "src=",
                "onerror=",
                "<!--",
            ],
        );
        assert_allowed_html(&rendered)?;
    }
    Ok(())
}

#[test]
fn inline_html_tags_and_attributes_are_omitted_but_markdown_text_survives() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "PUBLIC <span onclick=\"ATTRIBUTE_SECRET()\" style=\"background:url(ATTRIBUTE_SECRET)\">",
        "**VISIBLE_TEXT**</span> ",
        "<img src=\"https://resource.example.test/IMAGE_SECRET\"> AFTER"
    ))?;
    let rendered = document.render()?;
    assert!(rendered.html.contains("PUBLIC"));
    assert!(rendered.html.contains("<strong>VISIBLE_TEXT</strong>"));
    assert!(rendered.html.contains("AFTER"));
    assert_absent(
        &rendered,
        &[
            "ATTRIBUTE_SECRET",
            "IMAGE_SECRET",
            "resource.example.test",
            "<span",
            "<img",
            "onclick=",
            "src=",
        ],
    );
    assert_allowed_html(&rendered)
}

#[test]
fn unsafe_link_schemes_keep_labels_but_never_generate_href() -> TestResult {
    for url in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "jav&#x61;script:alert(1)",
        "file:///private/secret",
        "FiLe:///private/secret",
        "data:text/html;base64,PHNjcmlwdD4=",
        "DaTa:text/plain,secret",
        "vbscript:secret",
        "ftp://example.test/secret",
        "qrc:/secret",
        "kryptopass-spoiler:1:0",
        "//example.test/secret",
        "/relative/secret",
    ] {
        let mut document = Document::default();
        document.load(&format!("[VISIBLE_LABEL]({url})"))?;
        let rendered = document.render()?;
        assert!(
            rendered.html.contains("VISIBLE_LABEL"),
            "Lost label for {url}"
        );
        assert!(
            !rendered.html.contains("href="),
            "Unsafe URL {url} became a link: {}",
            rendered.html.as_str()
        );
        assert!(!rendered.html.contains("<a"));
        assert!(rendered.actions.is_empty());
        assert_allowed_html(&rendered)?;
    }
    Ok(())
}

#[test]
fn allowed_links_escape_attributes_and_autolinks_use_the_same_policy() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "[SECURE](https://example.test/?a=1&b=2) ",
        "[PLAIN](http://example.test/path) ",
        "[MAIL](mailto:user@example.test) ",
        "[MIXED](HTTPS://example.test/mixed) ",
        "[QUOTED](https://example.test/&quot;onclick=&quot;ATTRIBUTE_SECRET)\n\n",
        "https://autolink.example.test/path\n\n",
        "<javascript:AUTOLINK_SECRET> <file:///AUTOLINK_SECRET> <data:text/plain,AUTOLINK_SECRET>"
    ))?;
    let rendered = document.render()?;
    for href in [
        "href=\"https://example.test/?a=1&amp;b=2\"",
        "href=\"http://example.test/path\"",
        "href=\"mailto:user@example.test\"",
        "href=\"HTTPS://example.test/mixed\"",
        "href=\"https://autolink.example.test/path\"",
    ] {
        assert!(rendered.html.contains(href), "Missing allowed link {href}");
    }
    assert!(
        rendered
            .html
            .contains("&quot;onclick=&quot;ATTRIBUTE_SECRET")
    );
    assert_absent(
        &rendered,
        &[
            " onclick=\"",
            "href=\"javascript:",
            "href=\"file:",
            "href=\"data:",
        ],
    );
    assert_allowed_html(&rendered)
}

#[test]
fn gfm_tasks_tables_highlight_and_strikethrough_use_allowed_html() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "- [ ] PENDING_TASK\n- [x] DONE_TASK\n\n",
        "| LEFT | CENTER | RIGHT |\n| :--- | :---: | ---: |\n",
        "| **CELL_STRONG** | `CELL_CODE` | VALUE |\n\n",
        "~~REMOVED~~ ==MARKED **MARKED_STRONG**=="
    ))?;
    let rendered = document.render()?;
    assert!(rendered.html.contains("<li>☐ PENDING_TASK</li>"));
    assert!(rendered.html.contains("<li>☑ DONE_TASK</li>"));
    assert!(rendered.html.contains("<table "));
    assert!(rendered.html.contains("<thead>"));
    assert!(rendered.html.contains("<tbody>"));
    assert!(rendered.html.contains("<th align=\"left\">LEFT</th>"));
    assert!(rendered.html.contains("<th align=\"center\">CENTER</th>"));
    assert!(rendered.html.contains("<th align=\"right\">RIGHT</th>"));
    assert!(rendered.html.contains("<strong>CELL_STRONG</strong>"));
    assert!(rendered.html.contains("<code>CELL_CODE</code>"));
    assert!(rendered.html.contains("<s>REMOVED</s>"));
    assert!(rendered.html.contains("<span style=\"background-color:"));
    assert!(
        rendered
            .html
            .contains("MARKED <strong>MARKED_STRONG</strong></span>")
    );
    assert_absent(&rendered, &["<input", "<mark", "<del"]);
    assert_allowed_html(&rendered)
}

#[test]
fn double_underscore_is_strong_and_code_escapes_html_without_parsing_spoilers() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "__BOLD_TEXT__ `||INLINE_LITERAL|| <img> & value`\n\n",
        "```html\n||BLOCK_LITERAL|| <script>CODE_TEXT</script> & value\n```"
    ))?;
    let rendered = document.render()?;
    assert!(rendered.html.contains("<strong>BOLD_TEXT</strong>"));
    assert!(
        rendered
            .html
            .contains("<code>||INLINE_LITERAL|| &lt;img&gt; &amp; value</code>")
    );
    assert!(rendered.html.contains("<pre style=\"background-color:"));
    assert!(rendered.html.contains(
        "<code>||BLOCK_LITERAL|| &lt;script&gt;CODE_TEXT&lt;/script&gt; &amp; value\n</code></pre>"
    ));
    assert!(rendered.actions.is_empty());
    assert_absent(
        &rendered,
        &["<u>", "<img>", "<script>", "class=\"language-"],
    );
    assert_allowed_html(&rendered)
}

#[test]
fn commonmark_structure_and_revealed_spoilers_stay_in_allowed_html_subset() -> TestResult {
    let mut document = Document::default();
    document.load(concat!(
        "# HEADING\n\n> QUOTE *EMPHASIS*\n\n",
        "3. ORDERED\n4. NEXT\n\n- BULLET\n\n---\n\n",
        "LINE  \nBREAK\n\n",
        "||**SECRET_STRONG** ==SECRET_MARK== [SECRET_LINK](https://example.test/secret)||"
    ))?;
    let hidden = document.render()?;
    assert!(hidden.html.contains("<h1>HEADING</h1>"));
    assert!(hidden.html.contains("<blockquote>"));
    assert!(hidden.html.contains("<em>EMPHASIS</em>"));
    assert!(hidden.html.contains("<ol start=\"3\">"));
    assert!(hidden.html.contains("<ul>"));
    assert!(hidden.html.contains("<hr"));
    assert!(hidden.html.contains("<br"));
    assert_allowed_html(&hidden)?;
    assert!(document.toggle(&action_at(&hidden, 0)?));
    assert_allowed_html(&document.render()?)
}

#[test]
fn source_limit_accepts_exact_byte_boundary_including_multibyte_text() -> TestResult {
    for source in [
        "x".repeat(MAX_SOURCE_BYTES),
        "é".repeat(MAX_SOURCE_BYTES / 2),
    ] {
        assert_eq!(source.len(), MAX_SOURCE_BYTES);
        let mut document = Document::default();
        document.load(&source)?;
        let rendered = document.render()?;
        assert!(rendered.html.starts_with("<p>"));
        assert!(rendered.html.ends_with("</p>\n"));
        assert!(rendered.html.contains(source.as_str()));
        assert!(rendered.actions.is_empty());
        assert!(rendered.revealed_actions.is_empty());
    }
    Ok(())
}

#[test]
fn oversized_load_clears_previous_content_reveals_and_actions() -> TestResult {
    for oversized in [
        "x".repeat(MAX_SOURCE_BYTES + 1),
        "é".repeat(MAX_SOURCE_BYTES / 2 + 1),
    ] {
        assert!(oversized.len() > MAX_SOURCE_BYTES);
        let mut document = Document::default();
        document.load("PUBLIC_OLD ||SECRET_OLD||")?;
        let old_action = action_at(&document.render()?, 0)?;
        assert!(document.toggle(&old_action));
        assert!(document.render()?.html.contains("SECRET_OLD"));

        assert!(document.load(&oversized).is_err());
        assert!(!document.toggle(&old_action));
        let cleared = document.render()?;
        assert!(cleared.html.is_empty());
        assert!(cleared.actions.is_empty());
        assert!(cleared.revealed_actions.is_empty());

        document.load("||SECRET_NEW||")?;
        let fresh = document.render()?;
        assert_absent(&fresh, &["PUBLIC_OLD", "SECRET_OLD", "SECRET_NEW"]);
        assert!(!document.toggle(&old_action));
        assert!(document.toggle(&action_at(&fresh, 0)?));
        assert!(document.render()?.html.contains("SECRET_NEW"));
    }
    Ok(())
}

#[test]
fn hiding_parent_revokes_nested_action_and_resets_child_reveal() -> TestResult {
    // A link label gives Comrak a separate inline delimiter scope, so the inner
    // spoiler is a genuine descendant rather than another sibling spoiler.
    let mut document = Document::default();
    document.load(concat!(
        "||OUTER_BEFORE [||INNER_SECRET||](https://nested.example.test/PRIVATE_URL) ",
        "OUTER_AFTER||"
    ))?;
    let hidden = document.render()?;
    assert_eq!(
        hidden.actions.len(),
        1,
        "Only the outer spoiler is initially visible"
    );
    assert_absent(
        &hidden,
        &[
            "OUTER_BEFORE",
            "OUTER_AFTER",
            "INNER_SECRET",
            "nested.example.test",
            "PRIVATE_URL",
        ],
    );
    let outer = action_at(&hidden, 0)?;

    assert!(document.toggle(&outer));
    let outer_only = document.render()?;
    assert_eq!(
        outer_only.actions.len(),
        2,
        "Comrak must parse the child spoiler inside the link"
    );
    assert!(outer_only.html.contains("OUTER_BEFORE"));
    assert!(outer_only.html.contains("OUTER_AFTER"));
    assert_absent(
        &outer_only,
        &["INNER_SECRET", "nested.example.test", "PRIVATE_URL"],
    );
    let inner = outer_only
        .actions
        .iter()
        .find(|action| **action != outer)
        .cloned()
        .ok_or("Missing nested spoiler action")?;
    assert!(document.toggle(&inner));
    let both = document.render()?;
    assert!(both.html.contains("INNER_SECRET"));
    assert_eq!(both.revealed_actions, vec![outer.clone(), inner.clone()]);
    assert_allowed_html(&both)?;

    assert!(document.toggle(&outer));
    let hidden_again = document.render()?;
    assert_eq!(hidden_again.actions, vec![outer.clone()]);
    assert!(hidden_again.revealed_actions.is_empty());
    assert_absent(
        &hidden_again,
        &["OUTER_BEFORE", "OUTER_AFTER", "INNER_SECRET"],
    );
    assert!(!document.toggle(&inner));

    assert!(document.toggle(&outer));
    let reopened = document.render()?;
    assert_eq!(reopened.actions, outer_only.actions);
    assert_eq!(reopened.revealed_actions, vec![outer]);
    assert!(reopened.html.contains("OUTER_BEFORE"));
    assert_absent(
        &reopened,
        &["INNER_SECRET", "nested.example.test", "PRIVATE_URL"],
    );
    assert_allowed_html(&reopened)
}
