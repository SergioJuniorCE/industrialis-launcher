// Opt-in compatibility probe: compile with --release 8 and use the actual 1.7.10
// client jar on the classpath. No game startup, world, account or JAR modification.
package industrialis.acceptance;

import java.net.InetAddress;
import java.lang.reflect.Method;

public final class MinecraftServerAddress {
    public static void main(String[] args) throws Exception {
        if (args.length != 2) {
            throw new IllegalArgumentException("Usage: MinecraftServerAddress <CLI host> <CLI port>");
        }
        int expectedPort = Integer.parseInt(args[1]);
        Class<?> parser = Class.forName("bjm");
        Method parse = parser.getMethod("a", String.class);
        // This is the exact host/port composition in Forge's startup connection.
        Object result = parse.invoke(null, args[0] + ":" + expectedPort);
        String actualHost = (String) parser.getMethod("a").invoke(result);
        int actualPort = (Integer) parser.getMethod("b").invoke(result);
        System.out.println("Minecraft parsed host=" + actualHost + " port=" + actualPort);
        if (actualPort != expectedPort ||
            !InetAddress.getByName(args[0]).equals(InetAddress.getByName(actualHost))) {
            throw new AssertionError("Forge startup changed the intended server endpoint");
        }
        System.out.println("Forge startup endpoint preserved");
    }
}
