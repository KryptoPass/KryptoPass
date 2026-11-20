pragma Singleton
import QtQuick
import org.kde.ki18n

QtObject {
    id: palette

    // Store the dark value as a stable identity; theme changes only affect presentation.
    readonly property var options: [
        {
            value: "transparent",
            light: "transparent",
            name: KI18n.i18nc("@item folder color", "No color"),
            aliases: []
        },
        {
            value: "#da4453",
            light: "#f3a6ae",
            name: KI18n.i18nc("@item folder color", "Red"),
            aliases: ["#cf5b72"]
        },
        {
            value: "#d76d4b",
            light: "#f2b6a2",
            name: KI18n.i18nc("@item folder color", "Coral"),
            aliases: ["#f28c8c", "#f47750"]
        },
        {
            value: "#d56800",
            light: "#f1bd87",
            name: KI18n.i18nc("@item folder color", "Orange"),
            aliases: ["#e1a25b", "#f67400"]
        },
        {
            value: "#b88a20",
            light: "#ebd28e",
            name: KI18n.i18nc("@item folder color", "Amber"),
            aliases: ["#f4d66d", "#fdbc4b"]
        },
        {
            value: "#259653",
            light: "#a2d4b2",
            name: KI18n.i18nc("@item folder color", "Green"),
            aliases: ["#87c796", "#27ae60"]
        },
        {
            value: "#159d84",
            light: "#96d7c9",
            name: KI18n.i18nc("@item folder color", "Turquoise"),
            aliases: ["#80d5dc", "#1abc9c"]
        },
        {
            value: "#258ec0",
            light: "#9acde4",
            name: KI18n.i18nc("@item folder color", "Blue"),
            aliases: ["#82a9ee", "#3daee9"]
        },
        {
            value: "#7375d4",
            light: "#c0c1ef",
            name: KI18n.i18nc("@item folder color", "Indigo"),
            aliases: ["#5b5fc7"]
        },
        {
            value: "#9b59b6",
            light: "#d7b7e4",
            name: KI18n.i18nc("@item folder color", "Violet"),
            aliases: ["#a775ff"]
        },
        {
            value: "#e85d9e",
            light: "#f5b2d1",
            name: KI18n.i18nc("@item folder color", "Pink"),
            aliases: ["#e78bd6"]
        },
        {
            value: "#818d8e",
            light: "#c4cccd",
            name: KI18n.i18nc("@item folder color", "Gray"),
            aliases: ["#d8d8da", "#95a5a6"]
        }
    ]

    function optionFor(value: string): var {
        const normalized = value.toLowerCase();
        return options.find(option => option.value === normalized || option.light === normalized || option.aliases.includes(normalized));
    }

    function canonicalColor(value: string): string {
        const option = palette.optionFor(value);
        return option ? option.value : value;
    }

    function displayColor(value: string, darkTheme: bool): string {
        const option = palette.optionFor(value);
        return option ? (darkTheme ? option.value : option.light) : value || "transparent";
    }
}
