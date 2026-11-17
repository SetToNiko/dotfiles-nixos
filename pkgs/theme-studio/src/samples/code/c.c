#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_COLOURS 34
#define NAME_LEN 32

/* One palette entry: a name and its colour as 0xRRGGBB. */
struct colour {
    char name[NAME_LEN];
    uint32_t rgb;
};

struct palette {
    size_t len;
    struct colour colours[MAX_COLOURS];
};

static int parse_line(const char *line, struct colour *out) {
    char hex[8];
    if (sscanf(line, "%31s = #%6s", out->name, hex) != 2)
        return -EINVAL;
    char *end = NULL;
    unsigned long v = strtoul(hex, &end, 16);
    if (*end != '\0' || v > 0xffffff)
        return -ERANGE;
    out->rgb = (uint32_t)v;
    return 0;
}

// Relative luminance, close enough for picking a readable text colour.
static double luminance(uint32_t rgb) {
    double r = (rgb >> 16) / 255.0, g = ((rgb >> 8) & 0xff) / 255.0;
    double b = (rgb & 0xff) / 255.0;
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

int main(int argc, char **argv) {
    FILE *f = fopen(argc > 1 ? argv[1] : "palette.conf", "r");
    if (!f) {
        perror("fopen");
        return EXIT_FAILURE;
    }
    struct palette p = {0};
    char line[128];
    while (fgets(line, sizeof line, f) && p.len < MAX_COLOURS) {
        if (parse_line(line, &p.colours[p.len]) == 0)
            p.len++;
    }
    fclose(f);
    for (size_t i = 0; i < p.len; i++)
        printf("%-12s #%06x %s\n", p.colours[i].name, p.colours[i].rgb,
               luminance(p.colours[i].rgb) > 0.5 ? "light" : "dark");
    return 0;
}
