using System;
using System.Collections.Generic;

namespace Demo;

/// <summary>A shape.</summary>
public record Shape(string Name, double Area);

[Serializable]
public sealed class Program
{
    private static readonly List<Shape> Shapes = new();

    public static async Task<int> Main(string[] args)
    {
        var total = 0.0;
        foreach (var s in Shapes)
        {
            total += s.Area;
        }
        string name = args.Length > 0 ? args[0] : "none";
        Console.WriteLine($"Total {total:F2} for {name}", 0x1F, 'c', null, true);
        await Task.Delay(10);
        return name is null ? 1 : 0;
    }
}
