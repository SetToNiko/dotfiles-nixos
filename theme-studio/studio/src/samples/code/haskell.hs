{-# LANGUAGE OverloadedStrings #-}
-- Contrast between palette colours, as in WCAG 2.1.
module Contrast
  ( Rgb (..)
  , parseHex
  , contrast
  , readable
  ) where

import Data.Char (digitToInt, isHexDigit, toLower)
import Data.List (sortOn)
import qualified Data.Map.Strict as Map
import Text.Printf (printf)

data Rgb = Rgb !Int !Int !Int
  deriving (Eq, Show)

newtype Palette = Palette (Map.Map String Rgb)

-- | "#rrggbb" to a colour, or why not.
parseHex :: String -> Either String Rgb
parseHex ('#' : hex)
  | length hex == 6 && all isHexDigit hex =
      let [r, g, b] = map byte (chunks hex) in Right (Rgb r g b)
  | otherwise = Left ("not six hex digits: " ++ hex)
  where
    chunks (a : b : rest) = [a, b] : chunks rest
    chunks _ = []
    byte = foldl (\acc c -> acc * 16 + digitToInt (toLower c)) 0
parseHex other = Left ("missing #: " ++ other)

luminance :: Rgb -> Double
luminance (Rgb r g b) = 0.2126 * lin r + 0.7152 * lin g + 0.0722 * lin b
  where
    lin c =
      let s = fromIntegral c / 255
       in if s <= 0.03928 then s / 12.92 else ((s + 0.055) / 1.055) ** 2.4

contrast :: Rgb -> Rgb -> Double
contrast a b = (hi + 0.05) / (lo + 0.05)
  where
    (la, lb) = (luminance a, luminance b)
    (hi, lo) = (max la lb, min la lb)

-- | The colours readable on a background (4.5:1 or more), best first.
readable :: Rgb -> Palette -> [(String, Double)]
readable bg (Palette colours) =
  sortOn (negate . snd)
    [ (name, ratio)
    | (name, c) <- Map.toList colours
    , let ratio = contrast bg c
    , ratio >= 4.5
    ]

main :: IO ()
main = do
  let Right bg = parseHex "#282828"
      palette = Palette (Map.fromList [(k, c) | (k, Right c) <- [("fg0", parseHex "#ebdbb2"), ("red", parseHex "#fb4934")]])
  mapM_ (\(name, r) -> printf "%-8s %5.2f:1\n" name r) (readable bg palette)
