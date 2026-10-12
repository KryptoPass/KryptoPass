use super::format;

#[test]
fn numbered_arguments_follow_marker_order_and_repeat() {
    let arguments: Vec<_> = (1..=10).map(|index| format!("value-{index}")).collect();

    assert_eq!(
        format("fixture:%2|%1|%10|%2|%10|%1", &arguments),
        "fixture:value-2|value-1|value-10|value-2|value-10|value-1"
    );
}

#[test]
fn argument_contents_are_never_interpreted_as_markers() {
    let arguments = ["first contains %2", "second contains %1 and %2"].map(str::to_owned);

    assert_eq!(
        format("fixture:%1|%2|%1", &arguments),
        "fixture:first contains %2|second contains %1 and %2|first contains %2"
    );
}

#[test]
fn substitution_preserves_utf8_source_and_arguments() {
    let arguments = ["José", "東京 🔑"].map(str::to_owned);

    assert_eq!(
        format("Prueba única 🗝️ — %2, %1: contraseña", &arguments),
        "Prueba única 🗝️ — 東京 🔑, José: contraseña"
    );
}

#[test]
fn unknown_markers_and_literal_percentages_are_preserved() {
    let arguments = ["known", "second"].map(str::to_owned);

    assert_eq!(
        format("fixture %1: %0|%3|%10|%n|%x|100%|%%|tail%", &arguments),
        "fixture known: %0|%3|%10|%n|%x|100%|%%|tail%"
    );
}

#[test]
fn unavailable_and_overflowing_markers_are_preserved_without_arguments() {
    let source = "fixture:%1|%2|%10|%99999999999999999999999999999999999999";

    assert_eq!(format(source, &[]), source);
}
