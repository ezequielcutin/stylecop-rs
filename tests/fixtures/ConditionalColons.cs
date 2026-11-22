internal class ConditionalColons
{
    public int Before(bool condition, int first, int second)
        => condition ? (first): second;

    public int After(bool condition, int first, int second)
        => condition ? first :second;

    public int Wrapped(bool condition, int first, int second)
        => condition ? first
            : second;

    public int Nested(bool condition, bool inner, int first, int second, int third)
        => condition ? inner ? first : second: third;

    public string Interpolated(bool condition, int first, int second)
        => $"{(condition ? first : second)}";

    public string InterpolatedBefore(bool condition, int first, int second)
        => $"{(condition ? first: second)}";

    public string InterpolatedAfter(bool condition, int first, int second)
        => $"{(condition ? first :second)}";

    public int ArrayPattern(object value)
    {
        switch (value)
        {
            case var item when item is int? [] elements:
                return elements.Length;
            case var item when item as int? is null:
                return -1;
            case true ? 1: 2:
                return 1;
            default:
                return 0;
        }
    }
}
