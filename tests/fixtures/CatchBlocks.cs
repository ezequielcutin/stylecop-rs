using System;

class CatchBlocks
{
    void CommentOnly()
    {
        try
        {
            Work();
        }
        catch { /* The operation can be skipped. */ }
    }

    void Empty()
    {
        try
        {
            Work();
        }
        catch { }
    }

    void Filtered(bool retry)
    {
        try
        {
            Work();
        }
        catch (InvalidOperationException) when (retry) { Work(); }
    }

    void Typed()
    {
        try
        {
            Work();
        }
        catch (InvalidOperationException) { Work(); }
    }

    void Multiline()
    {
        try
        {
            Work();
        }
        catch
        {
            Work();
        }
    }

    void UntypedFilter(bool retry)
    {
        try
        {
            Work();
        }
        catch when (retry) { Work(); }
    }

    void Work()
    {
        throw new InvalidOperationException();
    }

    void when(bool retry) { Work(); }

    void LambdaFilter(int[] values)
    {
        try
        {
            Work();
        }
        catch when (Array.Exists(values, value => value > 0)) { Work(); }
    }
}
