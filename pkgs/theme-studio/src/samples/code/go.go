package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sort"
	"strings"
)

// Theme pairs a palette with a font preset.
type Theme struct {
	Name    string `json:"name"`
	Palette string `json:"palette"`
	Fonts   string `json:"fonts"`
}

// ErrNoThemes is returned when the file has none.
var ErrNoThemes = errors.New("no themes")

const maxThemes = 64

func loadThemes(path string) ([]Theme, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("reading %s: %w", path, err)
	}
	var file struct {
		Version int              `json:"version"`
		Themes  map[string]Theme `json:"themes"`
	}
	if err := json.Unmarshal(data, &file); err != nil {
		return nil, fmt.Errorf("parsing %s: %w", path, err)
	}
	if len(file.Themes) == 0 {
		return nil, ErrNoThemes
	}
	themes := make([]Theme, 0, len(file.Themes))
	for name, t := range file.Themes {
		t.Name = name
		themes = append(themes, t)
	}
	sort.Slice(themes, func(i, j int) bool { return themes[i].Name < themes[j].Name })
	return themes, nil
}

func main() {
	themes, err := loadThemes(os.ExpandEnv("$HOME/dotfiles-nixos/configs/themes.json"))
	if errors.Is(err, ErrNoThemes) {
		fmt.Println("nothing yet")
		return
	} else if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	for i, t := range themes[:min(len(themes), maxThemes)] {
		fmt.Printf("%2d: %-20s %s / %s\n", i, t.Name, t.Palette, strings.ToUpper(t.Fonts))
	}
}
