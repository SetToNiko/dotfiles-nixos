package studio.fonts;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Optional;

/** A font preset: one family per role, with sizes in points. */
public final class FontPreset {
    public enum Role { MONO, UI, BAR }

    public record Slot(Role role, String family, int size) {
        public Slot {
            if (size < 6 || size > 48) {
                throw new IllegalArgumentException("size out of range: " + size);
            }
        }
    }

    private final String name;
    private final List<Slot> slots = new ArrayList<>();

    public FontPreset(String name) {
        this.name = name;
    }

    public FontPreset with(Role role, String family, int size) {
        slots.removeIf(s -> s.role() == role);
        slots.add(new Slot(role, family, size));
        return this;
    }

    public Optional<Slot> slot(Role role) {
        return slots.stream().filter(s -> s.role() == role).findFirst();
    }

    // Writes the preset as fontconfig-style lines, mono first.
    public void save(Path dir) throws IOException {
        List<String> lines = new ArrayList<>();
        lines.add("# preset " + name);
        slots.stream()
            .sorted(Comparator.comparing(Slot::role))
            .forEach(s -> lines.add(s.role().name().toLowerCase() + " = " + s.family() + " " + s.size()));
        Files.write(dir.resolve(name + ".conf"), lines);
    }

    public static void main(String[] args) {
        var preset = new FontPreset("Work")
            .with(Role.MONO, "JetBrains Mono", 14)
            .with(Role.UI, "Inter", 11)
            .with(Role.BAR, "JetBrainsMono Nerd Font", 11);
        try {
            preset.save(Path.of(System.getProperty("user.home"), ".config", "fonts"));
            System.out.printf("saved %s (%d roles, 0x%x)%n", preset.name, preset.slots.size(), 42);
        } catch (IOException e) {
            System.err.println("couldn't save: " + e.getMessage());
            System.exit(1);
        }
    }
}
