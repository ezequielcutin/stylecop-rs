internal class InterpolationSpacing
{
    public string Describe(string? value) => value is null
        ? $"(Value: {value})"
        : $"{value}={value}";

    public string Format(int value) => $"{value:=}";

    public string DescribeNullable(string ? value) => $"{value}";
}
