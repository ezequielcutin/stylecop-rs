internal class PrefixOperators
{
    public int Change(int value, bool increment)
    {
        if (increment)
        {
            return ++value;
        }

        return --value;
    }

    public int Label(int value)
    {
        switch (value)
        {
            case -1:
                return -1;
            case - 2:
                return value;
            default:
                return value ++;
        }
    }
}
