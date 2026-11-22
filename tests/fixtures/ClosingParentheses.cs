class ClosingParentheses
{
    private int Choose(bool flag, int value, int fallback)
    {
        var ordinary = Choose(
            flag,
            value,
            fallback
            );
        var detachedCall = Choose
        (
            flag,
            ordinary,
            fallback
        );
        var conditional = flag ? (value): fallback;
        var nested = flag ? ((ordinary > 0) ? (ordinary): 0): fallback;

        var tuple =
        (
            Value: value,
            Fallback: fallback
        );
        var expressionTuple =
        (
            (int)ordinary,
            fallback
        );

        switch (conditional)
        {
            case (42):
                return detachedCall + nested;

            case (((4 > 2) ? 1 : 0)):
                return tuple.Value;
        }

        return expressionTuple.Item2;
    }

    private (int First, int Second) Pair()
    {
        return
        (
            1,
            2
        );
    }
}

class OtherUses
{
    string Format(int? number)
    {
        var name = $"{number?.ToString():X}";
        return $@"{number.ToString():
)X}";
    }

    bool Match(object value, bool flag)
    {
        switch (value)
        {
            case int n when value as int? == (0):
                return true;

            case int n when flag ? value as int? == 0 : (false):
                return true;
        }

        return false;
    }
}
