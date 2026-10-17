pragma Singleton
import QtQuick

QtObject {
    id: palette

    // Store the dark value as a stable identity; theme changes only affect presentation.
    readonly property var options: [
        {
            value: "transparent",
            light: "transparent",
            name: qsTr("Sin color"),
            aliases: []
        },
        {
            value: "#da4453",
            light: "#f3a6ae",
            name: qsTr("Rojo"),
            aliases: ["#cf5b72"]
        },
        {
            value: "#d76d4b",
            light: "#f2b6a2",
            name: qsTr("Coral"),
            aliases: ["#f28c8c", "#f47750"]
        },
        {
            value: "#d56800",
            light: "#f1bd87",
            name: qsTr("Naranja"),
            aliases: ["#e1a25b", "#f67400"]
        },
        {
            value: "#b88a20",
            light: "#ebd28e",
            name: qsTr("Ámbar"),
            aliases: ["#f4d66d", "#fdbc4b"]
        },
        {
            value: "#259653",
            light: "#a2d4b2",
            name: qsTr("Verde"),
            aliases: ["#87c796", "#27ae60"]
        },
        {
            value: "#159d84",
            light: "#96d7c9",
            name: qsTr("Turquesa"),
            aliases: ["#80d5dc", "#1abc9c"]
        },
        {
            value: "#258ec0",
            light: "#9acde4",
            name: qsTr("Azul"),
            aliases: ["#82a9ee", "#3daee9"]
        },
        {
            value: "#7375d4",
            light: "#c0c1ef",
            name: qsTr("Índigo"),
            aliases: ["#5b5fc7"]
        },
        {
            value: "#9b59b6",
            light: "#d7b7e4",
            name: qsTr("Violeta"),
            aliases: ["#a775ff"]
        },
        {
            value: "#e85d9e",
            light: "#f5b2d1",
            name: qsTr("Rosa"),
            aliases: ["#e78bd6"]
        },
        {
            value: "#818d8e",
            light: "#c4cccd",
            name: qsTr("Gris"),
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
