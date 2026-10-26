import QtQuick
import QtQuick.Shapes
import qs.Commons

Item {
  id: root

  property real iconSize: Style.font.icon
  property color color: Color.foreground

  width: iconSize
  height: iconSize
  implicitWidth: iconSize
  implicitHeight: iconSize

  Item {
    id: scaler
    anchors.centerIn: parent
    width: 192
    height: 192
    scale: Math.min(root.width, root.height) / 192.0
    transformOrigin: Item.Center

    Shape {
      anchors.fill: parent
      antialiasing: true
      layer.enabled: true
      layer.samples: 4

      // Outer chip frame and pins
      ShapePath {
        strokeWidth: 12
        strokeColor: root.color
        fillColor: "transparent"
        capStyle: ShapePath.RoundCap
        joinStyle: ShapePath.RoundJoin
        PathSvg {
          path: "M52 40 L140 40 C147 40 152 45 152 52 L152 140 C152 147 147 152 140 152 L52 152 C45 152 40 147 40 140 L40 52 C40 45 45 40 52 40 Z M72 18 L72 40 M96 18 L96 40 M120 18 L120 40 M72 152 L72 174 M96 152 L96 174 M120 152 L120 174 M18 72 L40 72 M18 96 L40 96 M18 120 L40 120 M152 72 L174 72 M152 96 L174 96 M152 120 L174 120"
        }
      }

      // Telemetry pulse line inside the die
      ShapePath {
        strokeWidth: 11
        strokeColor: root.color
        fillColor: "transparent"
        capStyle: ShapePath.RoundCap
        joinStyle: ShapePath.RoundJoin
        PathSvg {
          path: "M58 102 L76 102 L87 72 L105 120 L116 92 L134 92"
        }
      }
    }
  }
}
