from typing import Any

class JaqError(Exception):
    """Base exception for jaq-py errors."""

class JaqParseError(JaqError):
    """Error parsing a jaq filter."""

class JaqCompileError(JaqError):
    """Error compiling a jaq filter."""

class JaqRuntimeError(JaqError):
    """Error executing a jaq filter."""

class JaqJsonError(JaqError):
    """Error converting data to/from JSON."""

def compile(filter: str, args: dict[str, Any] | None = None) -> JaqProgram:
    """Compile a jaq filter string into a reusable JaqProgram.

    Args:
        filter: A jq/jaq filter expression (e.g., ".foo", ".[] | select(.x > 1)").
        args: Optional dict of variables to bind (e.g., {"a": 1} makes $a available).

    Returns:
        A compiled JaqProgram that can be executed with different inputs.

    Raises:
        JaqParseError: If the filter syntax is invalid.
        JaqCompileError: If the filter fails to compile.
        JaqJsonError: If args values cannot be converted to JSON.

    Example:
        >>> prog = jaq_py.compile(".name")
        >>> prog.input_value({"name": "Alice"}).first()
        'Alice'

        >>> prog = jaq_py.compile("$a + $b + .", args={"a": 100, "b": 20})
        >>> prog.input_value(3).first()
        123
    """

class JaqProgram:
    """A compiled jaq program ready to accept input."""

    @property
    def program_string(self) -> str:
        """The original filter string that was compiled."""

    def input_text(self, text: str, slurp: bool = False) -> JaqProgramWithInput:
        """Provide input as a raw JSON string (fast path, skips Python object traversal).

        Args:
            text: A JSON-encoded string, or multiple whitespace-separated JSON values if slurp=True.
            slurp: If True, read all JSON values into an array.

        Returns:
            A JaqProgramWithInput that can be executed with `first()` or `all()`.

        Raises:
            JaqJsonError: If the string is not valid JSON.
        """

    def input_value(self, value: Any) -> JaqProgramWithInput:
        """Provide input as a Python object (convenient, but slower for large data).

        Args:
            value: Any JSON-serializable Python object.

        Returns:
            A JaqProgramWithInput that can be executed with `first()` or `all()`.

        Raises:
            JaqJsonError: If the value cannot be converted to JSON.
        """

class JaqProgramWithInput:
    """A compiled jaq program with input, ready to execute."""

    @property
    def program_string(self) -> str:
        """The original filter string that was compiled."""

    def first(self) -> Any | None:
        """Execute the filter and return the first result.

        Returns:
            The first output value, or None if the filter produces no output.

        Raises:
            JaqRuntimeError: If an error occurs during filter execution.
        """

    def first_text(self) -> str | None:
        """Execute the filter and return the first result as a JSON string.

        Returns:
            The first output value serialized as JSON, or None if no output.

        Raises:
            JaqRuntimeError: If an error occurs during filter execution.
        """

    def all(self) -> list[Any]:
        """Execute the filter and return all results as a list.

        Returns:
            A list of all output values produced by the filter.

        Raises:
            JaqRuntimeError: If an error occurs during filter execution.
        """

    def all_text(self) -> list[str]:
        """Execute the filter and return all results as JSON strings.

        This is the fastest output path — it skips Python object creation entirely
        and serializes each output value directly to a JSON string.

        Returns:
            A list of JSON-encoded strings, one per output value.

        Raises:
            JaqRuntimeError: If an error occurs during filter execution.
        """
