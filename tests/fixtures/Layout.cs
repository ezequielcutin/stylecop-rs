
class Layout
{
    int singleA;
    int singleB;

    int multi
        = 1;
    int afterMulti;

    /// <summary>Header.</summary>

    void DocThenBlank()
    {
    }
    void NoGap()
    {
        lock (this) {
            return;
        }

        if (true) { return; }

        if (true)
            return;

        if (true)
            Call(
                1);

        if (true)
        {
            return;
        }
        else
            return;

        if (true)
        {
            return;
        }

        else
        {
            return;
        }

        try
        {
            return;
        }

        catch
        {
            return;
        }

        finally
        {
            return;
        }

        do
        {
            return;
        }

        while (false);

        int x = 1;


        int spaced = 2;
        // kept

        x = 2;
        // note
        x = 3;
    }

    public int Mixed
    {
        get { return 1; }
        set
        {
            value = value;
        }
    }

    void Open()

    {
        int y = 1;

    }

    void Close()
    {
        int z = 1;
    }
    void AfterClose()
    {
    }
    /// <summary>Stuck.</summary>
    void StuckDoc()
    {
    }

    void Tiny() { }

    void BlankAfterOpen()
    {

        int q = 1;
    }
}


class Next
{
    void Call(int value)
    {
    }
}

