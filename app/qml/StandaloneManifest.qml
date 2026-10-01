import QtQuick

QtObject {
  property var source: ({})
  readonly property var value: ({
    id: String(source.id || "supaomamail"),
    name: String(source.name || "SupaOmaMail"),
    version: String(source.version || ""),
    barWidget: source.barWidget || ({defaults: ({})})
  })
}
