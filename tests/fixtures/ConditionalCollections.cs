public class ConditionalCollections
{
    public void Check(bool condition, bool values, bool flag, bool a, bool b, int[]? items, int[]? c, int y, int z, int e, int f, int fallback)
    {
        int[] pick = condition?[1]: [2];
        int get = items?[0];
        int alt = flag ? items?[0] : fallback;
        int both = flag ? items?[0] : c?[1];
        int nested = a ? b ? c?[0] : e : f;
        int[] spare = a ? b?[1]: [2] : [3];
        int sum = values?[0] + y : z;
        int[] inside = flag?[a ? 1 : 2]: [3];
        int[] paren = a ? (b?[1]: [2]) : [3];
        int kept = a ? (items?[0]) : 1;
        int[] tight = condition?[1]:[2];
        int outer = flag? items?[0] :1;
    }

    public T? Constrained<T>(int[]? items)
        where T : class
    {
        return items?[0] as T;
    }
}
