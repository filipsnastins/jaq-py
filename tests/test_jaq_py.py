import pytest

import jaq_py


def test_input_value_first() -> None:
    program = jaq_py.compile(".name").input_value({"name": "Alice"})
    assert program.first() == "Alice"

    program = jaq_py.compile(".[]").input_value([])
    assert program.first() is None


def test_input_value_all() -> None:
    program = jaq_py.compile(".[]").input_value([1, 2, 3])
    assert program.all() == [1, 2, 3]

    program = jaq_py.compile(".[]").input_value([])
    assert program.all() == []


def test_input_text_first() -> None:
    program = jaq_py.compile(".name").input_text('{"name": "Bob"}')

    assert program.first() == "Bob"


def test_input_text_all() -> None:
    program = jaq_py.compile(".[]").input_text("[1, 2, 3]")

    assert program.all() == [1, 2, 3]


def test_input_text_slurp() -> None:
    assert jaq_py.compile(".").input_text("1\n2\n3", slurp=True).first() == [1, 2, 3]

    assert jaq_py.compile(".[0]").input_text('{"a":1}\n{"b":2}', slurp=True).first() == {"a": 1}

    assert jaq_py.compile("map(.x)").input_text('{"x":1}\n{"x":2}', slurp=True).first() == [1, 2]


def test_output_text() -> None:
    assert jaq_py.compile(".").input_value("42").first_text() == '"42"'

    assert jaq_py.compile(".").input_value({"a": 1}).first_text() == '{"a":1}'

    assert jaq_py.compile(".[]").input_value([]).first_text() is None


def test_compile_with_args() -> None:
    program = jaq_py.compile("$a + $b + .", args={"a": 100, "b": 20})

    assert program.input_value(3).first() == 123


def test_program_string() -> None:
    assert jaq_py.compile(".id").program_string == ".id"

    assert jaq_py.compile(".name").input_value({}).program_string == ".name"


def test_parse_error() -> None:
    with pytest.raises(jaq_py.JaqParseError):
        jaq_py.compile(".foo |")


def test_compile_error() -> None:
    with pytest.raises(jaq_py.JaqCompileError, match=r"undefined_func"):
        jaq_py.compile("undefined_func")


def test_runtime_error() -> None:
    with pytest.raises(jaq_py.JaqRuntimeError, match=r"not an object"):
        jaq_py.compile(".foo").input_value("not an object").first()


def test_json_error_input_value() -> None:
    class NotSerializable:
        pass

    with pytest.raises(jaq_py.JaqJsonError, match=r"Cannot convert NotSerializable to jaq value"):
        jaq_py.compile(".").input_value(NotSerializable()).first()


def test_json_error_input_text() -> None:
    with pytest.raises(jaq_py.JaqJsonError, match=r"Failed to parse JSON:.*value expected"):
        jaq_py.compile(".").input_text("not valid json").first()


def test_json_error_compile_args() -> None:
    class NotSerializable:
        pass

    with pytest.raises(jaq_py.JaqJsonError, match=r"Cannot convert NotSerializable to jaq value"):
        jaq_py.compile("$x", args={"x": NotSerializable()})
