# frozen_string_literal: true

require "json"
require "fileutils"

# A colour in a palette, with the keys it is derived from.
class Colour
  attr_reader :key, :hex, :from

  HEX = Regexp.new("\\A#\\h{6}\\z")

  def initialize(key, hex, from: [])
    raise ArgumentError, "bad colour for #{key}: #{hex.inspect}" unless hex.match?(HEX)

    @key = key.to_sym
    @hex = hex.downcase
    @from = from.map(&:to_sym)
  end

  def rgb
    @hex.delete_prefix("#").scan(/../).map { |c| c.to_i(16) }
  end

  def mix(other, amount = 0.5)
    channels = rgb.zip(other.rgb).map { |a, b| (a + (b - a) * amount).round }
    Colour.new(:"#{key}_#{other.key}", format("#%02x%02x%02x", *channels), from: [key, other.key])
  end

  def to_s = "#{key}=#{hex}"
end

class Palette
  include Enumerable

  def self.load(path)
    data = JSON.parse(File.read(path))
    new(data.fetch("name", File.basename(path, ".json")), data.fetch("colours", {}))
  rescue JSON::ParserError, Errno::ENOENT => e
    warn "palette: #{e.message}"
    nil
  end

  def initialize(name, colours)
    @name = name
    @colours = colours.to_h { |k, v| [k.to_sym, Colour.new(k, v)] }
  end

  def each(&block) = @colours.each_value(&block)

  def save(dir)
    FileUtils.mkdir_p(dir)
    File.write(File.join(dir, "#{@name}.json"), JSON.pretty_generate(map { [_1.key, _1.hex] }.to_h))
  end
end

palette = Palette.load(ARGV.fetch(0, "gruvbox.json")) or exit 1
bg = palette.find { |c| c.key == :bg0 }
puts palette.reject { |c| c == bg }.map { |c| c.mix(bg, 0.25) }.first(3).join(", ")
