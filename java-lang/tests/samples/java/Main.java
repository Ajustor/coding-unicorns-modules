package org.example;

import java.util.List;
import java.util.function.Function;

/**
 * Entry point.
 * @author me
 */
@SuppressWarnings("unchecked")
public final class Main<T extends Comparable<T>> {
    private static final int MAX = 0x7F;

    public record Pair(String key, int value) {}

    public static void main(String[] args) throws Exception {
        List<Pair> pairs = List.of(new Pair("a", 1));
        Function<Integer, Integer> twice = n -> n * 2;
        for (var p : pairs) {
            System.out.printf("%s=%d%n", p.key(), twice.apply(p.value()));
        }
        String text = """
            Text block with "quotes"
            """;
        char c = '\n';
        if (args.length == 0 && text != null) { throw new IllegalStateException("no args"); }
    }
}
