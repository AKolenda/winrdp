# Win RDP branding

- `logo.svg`, `logo-32.svg`, `logo-16.svg`: the app logo masters (the teal ribbon W). The
  32 and 16 px masters are drawn on the pixel grid for small icon sizes.
- `social-preview.svg`: editable source for the repository share image.
- `build.py`: renders the masters to every copy. Edit the masters, never the copies, then run
  `python3 docs/branding/build.py`. It writes the launcher UI and website SVGs, the Tauri and
  session window icon (`src-tauri/icons/icon.png`), the hicolor PNGs in `packaging/icons/`,
  the website `favicon.ico`, `app-icon.png` (the README logo) and `social-preview.png`
  (1280 × 640, also served by the website).

To update the repository link preview, open https://github.com/AKolenda/winrdp/settings,
then choose **Social preview → Edit → Upload an image** and upload `social-preview.png`.
This controls shared-link previews; the account avatar beside the repository owner
continues to belong to the GitHub account.
