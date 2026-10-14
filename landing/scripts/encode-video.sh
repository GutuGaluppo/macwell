#!/usr/bin/env bash
# Usage: scripts/encode-video.sh <input.mov|mp4> <name>
# Produces public/media/video/<name>.mp4, <name>.webm, <name>-poster.avif/.jpg (needs: brew install ffmpeg).
# Source should be a 6-10 s screen capture of the 1200x780 app window at 60 fps.
set -euo pipefail
in="$1"; name="$2"; out="$(dirname "$0")/../public/media/video"
mkdir -p "$out"
vf="scale=1200:-2:flags=lanczos,fps=30"
ffmpeg -y -i "$in" -an -vf "$vf" -c:v libx264 -preset slow -crf 26 -pix_fmt yuv420p -movflags +faststart "$out/$name.mp4"
ffmpeg -y -i "$in" -an -vf "$vf" -c:v libvpx-vp9 -b:v 0 -crf 36 -row-mt 1 "$out/$name.webm"
ffmpeg -y -ss 1.5 -i "$in" -frames:v 1 -vf "scale=1200:-2" "$out/$name-poster.jpg"
ffmpeg -y -i "$out/$name-poster.jpg" -c:v libaom-av1 -crf 34 -still-picture 1 "$out/$name-poster.avif" || echo "AV1 poster skipped (ffmpeg build without libaom)"
ls -lh "$out/$name".* "$out/$name-poster".*
