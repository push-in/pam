#!/usr/bin/env python3
"""Return the center of a system launcher ANR's Wait button, if present."""

import re
import sys
import xml.etree.ElementTree as ET


def wait_point(dump: str) -> tuple[int, int] | None:
    start = dump.find("<hierarchy")
    if start < 0:
        return None
    try:
        root = ET.fromstring(dump[start:])
    except ET.ParseError:
        return None

    launcher_titles = {
        "Quickstep isn't responding",
        "Pixel Launcher isn't responding",
    }
    if not any(
        node.get("package") == "android"
        and node.get("resource-id") == "android:id/alertTitle"
        and node.get("text") in launcher_titles
        for node in root.iter("node")
    ):
        return None

    for node in root.iter("node"):
        if node.get("package") != "android" or node.get("text") != "Wait":
            continue
        match = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.get("bounds", ""))
        if match is None:
            continue
        left, top, right, bottom = map(int, match.groups())
        if right > left and bottom > top:
            return ((left + right) // 2, (top + bottom) // 2)
    return None


if __name__ == "__main__":
    point = wait_point(sys.stdin.read())
    if point is not None:
        print(*point)
