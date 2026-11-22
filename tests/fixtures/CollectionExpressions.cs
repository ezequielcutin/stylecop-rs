class CollectionExpressions
{
    int[] values = [];

    void Update()
    {
        values ??= [];
        values [0] = 1;
    }
}
