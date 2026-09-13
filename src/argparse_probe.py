"""Process-local argparse probe embedded in the shmart binary.

This file is executed only after explicit user consent. It deliberately uses
argparse private fields in this one compatibility boundary and never imports or
calls code merely to stringify parser values.
"""

import argparse
import datetime
import hashlib
import importlib.util
import json
import math
import os
import runpy
import sys


PROTOCOL_VERSION = "1.0"
CAPTURED_EXIT = 86
FAILED_EXIT = 87
MAX_STRING = 4096
MAX_ITEMS = 512
MAX_DEPTH = 12

_request = None
_result_fd = None
_captured = False


def _text(value, limit=MAX_STRING):
    if type(value) is not str:
        return None
    return value[:limit]


def _class_name(value):
    cls = type(value)
    module = cls.__module__ if type(cls.__module__) is str else "unknown"
    name = cls.__qualname__ if type(cls.__qualname__) is str else cls.__name__
    return (module + "." + name)[:MAX_STRING]


def _read_attr(value, name, default=None):
    try:
        attributes = object.__getattribute__(value, "__dict__")
        if type(attributes) is dict:
            return attributes.get(name, default)
        return default
    except BaseException:
        return default


def _simple(value):
    if value is None or type(value) is bool or type(value) is int:
        return value
    if type(value) is float:
        return value if math.isfinite(value) else None
    if type(value) is str:
        return value[:MAX_STRING]
    return None


def _safe_sequence(value):
    if type(value) is range:
        value = list(value[:MAX_ITEMS])
    elif type(value) is dict:
        value = list(value.keys())[:MAX_ITEMS]
    elif type(value) not in (list, tuple):
        return None
    result = []
    for item in value[:MAX_ITEMS]:
        simple = _simple(item)
        if simple is not None or item is None:
            result.append(simple)
        else:
            result.append({"type": _class_name(item)})
    return result


def _type_name(value):
    for known, name in (
        (str, "builtins.str"),
        (int, "builtins.int"),
        (float, "builtins.float"),
        (bool, "builtins.bool"),
    ):
        if value is known:
            return name
    return _class_name(value) if value is not None else None


def _nargs(value):
    names = {
        argparse.OPTIONAL: "OPTIONAL",
        argparse.ZERO_OR_MORE: "ZERO_OR_MORE",
        argparse.ONE_OR_MORE: "ONE_OR_MORE",
        argparse.REMAINDER: "REMAINDER",
        argparse.PARSER: "PARSER",
        argparse.SUPPRESS: "SUPPRESS",
    }
    if type(value) is str and value in names:
        return names[value]
    if value is None or type(value) is int:
        return value
    return _text(value)


def _action_kind(action):
    known = (
        (argparse._StoreAction, "store"),
        (argparse._StoreConstAction, "store_const"),
        (argparse._StoreTrueAction, "store_true"),
        (argparse._StoreFalseAction, "store_false"),
        (argparse._AppendAction, "append"),
        (argparse._AppendConstAction, "append_const"),
        (argparse._CountAction, "count"),
        (argparse._HelpAction, "help"),
        (argparse._VersionAction, "version"),
        (argparse._SubParsersAction, "subparsers"),
    )
    extend = getattr(argparse, "_ExtendAction", None)
    if extend is not None and isinstance(action, extend):
        return "extend"
    for cls, name in known:
        if isinstance(action, cls):
            return name
    return "custom"


def _sensitive(action):
    fragments = (
        "password",
        "passwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "credential",
        "private_key",
    )
    values = [_read_attr(action, "dest"), _read_attr(action, "help")]
    options = _read_attr(action, "option_strings", [])
    if type(options) in (list, tuple):
        values.extend(options[:MAX_ITEMS])
    combined = " ".join(value.lower() for value in values if type(value) is str)
    return any(fragment in combined for fragment in fragments)


def _default(value, redacted):
    if value is argparse.SUPPRESS:
        return {"present": False, "value": None, "redacted": False}
    if redacted:
        return {"present": True, "value": None, "redacted": True}
    simple = _simple(value)
    if simple is not None or value is None:
        return {"present": True, "value": simple, "redacted": False}
    return {
        "present": True,
        "value": None,
        "redacted": False,
        "type": _class_name(value),
    }


def _new_id(state, prefix, value):
    key = id(value)
    table = state[prefix]
    if key not in table:
        table[key] = prefix + str(len(table))
    return table[key]


def _serialize_action(action, state, group_ids):
    options = _read_attr(action, "option_strings", [])
    if type(options) not in (list, tuple):
        options = []
    options = [_text(item) for item in options[:MAX_ITEMS] if type(item) is str]
    redacted = _sensitive(action)
    choices = None if redacted else _safe_sequence(_read_attr(action, "choices"))
    metavar = _read_attr(action, "metavar")
    if type(metavar) in (list, tuple):
        metavar = [_text(item) for item in metavar[:MAX_ITEMS] if type(item) is str]
    else:
        metavar = _text(metavar)
    action_kind = _action_kind(action)
    record = {
        "id": _new_id(state, "a", action),
        "kind": "optional" if options else "positional",
        "option_strings": options,
        "dest": _text(_read_attr(action, "dest")),
        "action_kind": action_kind,
        "action_class": _class_name(action) if action_kind == "custom" else None,
        "required": _read_attr(action, "required") is True,
        "nargs": _nargs(_read_attr(action, "nargs")),
        "metavar": metavar,
        "help": _text(_read_attr(action, "help")),
        "choices": choices,
        "choices_redacted": redacted and _read_attr(action, "choices") is not None,
        "type_name": _type_name(_read_attr(action, "type")),
        "default": _default(_read_attr(action, "default"), redacted),
        "deprecated": _read_attr(action, "deprecated") is True,
        "group_ids": group_ids,
    }
    return record


def _serialize_parser(parser, state, depth=0):
    if depth > MAX_DEPTH:
        raise ValueError("parser nesting exceeds the probe limit")
    parser_key = id(parser)
    if parser_key in state["active"]:
        raise ValueError("recursive parser graph")
    state["active"].add(parser_key)

    actions = _read_attr(parser, "_actions", [])
    if type(actions) not in (list, tuple) or len(actions) > MAX_ITEMS:
        raise ValueError("invalid or oversized parser action list")
    groups = _read_attr(parser, "_action_groups", [])
    if type(groups) not in (list, tuple):
        groups = []
    mutexes = _read_attr(parser, "_mutually_exclusive_groups", [])
    if type(mutexes) not in (list, tuple):
        mutexes = []

    memberships = {}
    group_records = []
    for group in groups[:MAX_ITEMS]:
        group_id = _new_id(state, "g", group)
        group_actions = _read_attr(group, "_group_actions", [])
        if type(group_actions) not in (list, tuple):
            group_actions = []
        action_ids = []
        for action in group_actions[:MAX_ITEMS]:
            action_id = _new_id(state, "a", action)
            action_ids.append(action_id)
            memberships.setdefault(id(action), []).append(group_id)
        group_records.append(
            {
                "id": group_id,
                "title": _text(_read_attr(group, "title")),
                "description": _text(_read_attr(group, "description")),
                "argument_ids": action_ids,
            }
        )

    mutex_records = []
    for group in mutexes[:MAX_ITEMS]:
        group_id = _new_id(state, "m", group)
        group_actions = _read_attr(group, "_group_actions", [])
        if type(group_actions) not in (list, tuple):
            group_actions = []
        mutex_records.append(
            {
                "id": group_id,
                "required": _read_attr(group, "required") is True,
                "argument_ids": [
                    _new_id(state, "a", action) for action in group_actions[:MAX_ITEMS]
                ],
            }
        )

    argument_records = [
        _serialize_action(action, state, memberships.get(id(action), []))
        for action in actions
    ]

    subcommands = []
    for action in actions:
        if not isinstance(action, argparse._SubParsersAction):
            continue
        choices = _read_attr(action, "choices", {})
        if type(choices) is not dict or len(choices) > MAX_ITEMS:
            raise ValueError("invalid or oversized subcommand mapping")
        ordered = []
        by_parser = {}
        for name, child in list(choices.items())[:MAX_ITEMS]:
            if type(name) is not str or not isinstance(child, argparse.ArgumentParser):
                continue
            key = id(child)
            if key not in by_parser:
                entry = {"name": name[:MAX_STRING], "aliases": [], "parser": child}
                by_parser[key] = entry
                ordered.append(entry)
            else:
                by_parser[key]["aliases"].append(name[:MAX_STRING])
        for entry in ordered:
            subcommands.append(
                {
                    "name": entry["name"],
                    "aliases": entry["aliases"],
                    "parser": _serialize_parser(entry["parser"], state, depth + 1),
                }
            )

    record = {
        "id": _new_id(state, "p", parser),
        "prog": _text(_read_attr(parser, "prog")),
        "usage": _text(_read_attr(parser, "usage")),
        "description": _text(_read_attr(parser, "description")),
        "epilog": _text(_read_attr(parser, "epilog")),
        "prefix_chars": _text(_read_attr(parser, "prefix_chars")),
        "allow_abbrev": _read_attr(parser, "allow_abbrev") is True,
        "arguments": argument_records,
        "groups": group_records,
        "mutually_exclusive_groups": mutex_records,
        "subcommands": subcommands,
    }
    state["active"].remove(parser_key)
    return record


def _hash_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as source:
        while True:
            chunk = source.read(65536)
            if not chunk:
                break
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def _target_runtime():
    runtime = {
        "python_executable": os.path.realpath(sys.executable),
        "implementation": sys.implementation.name,
        "python_version": [sys.version_info.major, sys.version_info.minor],
        "sys_prefix": os.path.realpath(sys.prefix),
    }
    if _request["mode"] == "script":
        runtime["script_hash"] = _hash_file(_request["script_path"])
    else:
        spec = importlib.util.find_spec(_request["module_name"])
        origin = None if spec is None else spec.origin
        runtime["module_origin"] = _text(origin)
        if type(origin) is str and os.path.isfile(origin):
            runtime["module_origin_hash"] = _hash_file(origin)
    return runtime


def _write_message(message):
    payload = json.dumps(
        message, ensure_ascii=False, separators=(",", ":"), allow_nan=False
    ).encode("utf-8")
    maximum = int(_request["limits"]["schema_bytes"])
    if len(payload) > maximum:
        payload = json.dumps(
            {
                "protocol_version": PROTOCOL_VERSION,
                "request_id": _request["request_id"],
                "ok": False,
                "error": {"code": "SCHEMA_TOO_LARGE", "message": "schema limit exceeded"},
            },
            separators=(",", ":"),
        ).encode("utf-8")
    frame = len(payload).to_bytes(4, "big") + payload
    offset = 0
    while offset < len(frame):
        written = os.write(_result_fd, frame[offset:])
        if written <= 0:
            raise OSError("probe result pipe closed")
        offset += written


def _fail(code, message):
    try:
        _write_message(
            {
                "protocol_version": PROTOCOL_VERSION,
                "request_id": _request["request_id"],
                "ok": False,
                "error": {"code": code, "message": _text(message, 1024)},
            }
        )
    finally:
        os._exit(FAILED_EXIT)


def _capture(parser, method_name):
    global _captured
    if _captured:
        os._exit(FAILED_EXIT)
    _captured = True
    try:
        state = {"a": {}, "g": {}, "m": {}, "p": {}, "active": set()}
        runtime = _target_runtime()
        if _request["mode"] == "script":
            expected = _request.get("expected_target_hash")
            if expected != runtime.get("script_hash"):
                _fail("FINGERPRINT_MISMATCH", "script changed before probing")
        schema = {
            "schema_version": "1.0",
            "request_id": _request["request_id"],
            "source": "python-argparse-probe",
            "capture_method": method_name,
            "confidence": "partial" if "known" in method_name else "complete",
            "captured_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "target": {
                "mode": _request["mode"],
                "display": _request["display"],
                "canonical_identity_hash": _request["fingerprint"],
            },
            "runtime": runtime,
            "parser": _serialize_parser(parser, state),
            "warnings": [],
        }
        _write_message(
            {
                "protocol_version": PROTOCOL_VERSION,
                "request_id": _request["request_id"],
                "ok": True,
                "schema": schema,
            }
        )
    except BaseException as error:
        _fail("SCHEMA_INVALID", _class_name(error))
    os._exit(CAPTURED_EXIT)


def _install_patch():
    for name in (
        "parse_args",
        "parse_known_args",
        "parse_intermixed_args",
        "parse_known_intermixed_args",
    ):
        if hasattr(argparse.ArgumentParser, name):
            def intercepted(self, *args, __name=name, **kwargs):
                _capture(self, __name)
            setattr(argparse.ArgumentParser, name, intercepted)


def main():
    global _request, _result_fd
    raw = sys.stdin.buffer.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024:
        os._exit(FAILED_EXIT)
    try:
        _request = json.loads(raw.decode("utf-8"))
        if _request.get("protocol_version") != PROTOCOL_VERSION:
            os._exit(FAILED_EXIT)
        _result_fd = int(_request["result_fd"])
        os.set_inheritable(_result_fd, False)
        if _request["mode"] not in ("script", "module"):
            _fail("PROTOCOL_ERROR", "unsupported target mode")
        _install_patch()
        sys.argv = [_request["argv0"], *_request["target_args"]]
        if _request["mode"] == "script":
            sys.path[0] = os.path.dirname(_request["script_path"])
            runpy.run_path(_request["script_path"], run_name="__main__")
        else:
            sys.path[0] = os.getcwd()
            runpy.run_module(
                _request["module_name"], run_name="__main__", alter_sys=True
            )
        _fail("TARGET_EXITED_BEFORE_PARSE", "target exited before calling argparse")
    except SystemExit:
        _fail("TARGET_EXITED_BEFORE_PARSE", "target exited before calling argparse")
    except BaseException as error:
        _fail("TARGET_EXCEPTION_BEFORE_PARSE", _class_name(error))


if __name__ == "__main__":
    main()
