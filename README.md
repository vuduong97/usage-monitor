# Clawd

Ứng dụng nhỏ cho máy Mac và Windows: tự tìm đồng hồ Clawd trong mạng WiFi nhà bạn,
rồi hiện lên đồng hồ mức bạn đã dùng Claude (phiên 5 giờ và cả tuần). Khi bạn dùng
Claude Code, chú cua Clawd trên đồng hồ cũng nhảy theo: gõ phím khi Claude sửa code,
nhìn quanh khi Claude hỏi bạn, ngủ khi bạn rời máy.

Không cần nhập địa chỉ IP, không cần sửa file cấu hình.

## Cần có

- Đồng hồ Clawd (đã được cài sẵn phần mềm clawd).
- Máy Mac (macOS 11 trở lên) hoặc Windows 10/11, **cùng mạng WiFi với đồng hồ**.
- Ít nhất một coding agent trên máy đó: Claude Code, pi, Codex CLI, Gemini CLI, Cursor, opencode
  hoặc Copilot CLI. Muốn đồng hồ hiện mức dùng thì cần Claude Code đã đăng nhập (Pro/Max) hoặc
  Codex CLI (gói Plus/Pro).

## Bước 1: cho đồng hồ vào WiFi nhà bạn

1. Cắm điện cho đồng hồ. Lần đầu nó hiện **SETUP MODE** kèm tên một mạng WiFi
   (ví dụ `Clawd-3FA2`) và mật khẩu.
2. Trên điện thoại, vào **Cài đặt → WiFi**, chọn mạng đó, nhập mật khẩu trên màn hình.
3. Một trang cài đặt tự mở ra. Nếu không, mở trình duyệt và vào `http://192.168.4.1`.
   Nếu trang hỏi tên đăng nhập, dùng `admin` và mật khẩu ghi trên tấm thẻ đi kèm đồng hồ.
4. Chọn WiFi nhà bạn, nhập mật khẩu WiFi, bấm **Lưu**. Đồng hồ khởi động lại và
   hiện tên WiFi cùng địa chỉ IP của nó trong vài giây.

> WiFi 5 GHz không dùng được: đồng hồ chỉ bắt sóng 2.4 GHz. Router hai băng tần thường
> vẫn có một mạng 2.4 GHz.

## Bước 2: cài ứng dụng

Tải bản mới nhất ở trang **Releases** của repo này:

| Máy | Tải file |
| --- | --- |
| Mac chip Apple (M1, M2, M3...) | `Clawd-x.y.z-mac-arm64.dmg` |
| Mac chip Intel | `Clawd-x.y.z-mac-x86_64.dmg` |
| Windows | `Clawd-x.y.z-windows-setup.exe` |

Không biết Mac của mình chip gì? Bấm  → **Giới thiệu về máy Mac**: dòng *Chip* ghi
"Apple M..." là chip Apple, dòng *Bộ xử lý* ghi "Intel" là chip Intel.

### Mac

1. Mở file `.dmg`, kéo **Clawd** vào thư mục **Applications**.
2. Mở **Applications**, nhấp đúp **Clawd**. macOS sẽ báo *"Clawd" chưa được mở / không
   thể xác minh nhà phát triển* (ứng dụng chưa đăng ký với Apple, hoàn toàn bình thường
   với ứng dụng làm tặng bạn bè). Bấm **Xong**.
3. Vào **Cài đặt hệ thống → Quyền riêng tư & Bảo mật**, kéo xuống, bấm **Vẫn mở**
   cạnh dòng nói về Clawd, rồi nhập mật khẩu máy. Chỉ cần làm một lần.
4. Nếu macOS hỏi *cho phép Clawd tìm thiết bị trong mạng cục bộ*, bấm **Cho phép**.
   Nếu hỏi quyền đọc **Chuỗi khoá** (Keychain) cho mục "Claude Code-credentials", bấm
   **Luôn cho phép**: đó là chỗ Claude Code cất thông tin đăng nhập, Clawd chỉ đọc để
   hỏi Anthropic mức dùng của bạn.

Cách khác cho bước 2–3 (dùng Terminal):

```sh
xattr -dr com.apple.quarantine /Applications/Clawd.app
```

### Windows

1. Chạy file `Clawd-...-setup.exe`. Windows có thể hiện **Windows đã bảo vệ PC của bạn**
   (SmartScreen). Bấm **Thông tin khác** → **Vẫn chạy**.
2. Bấm **Next/Install**. Không cần quyền quản trị.
3. Nếu Windows Firewall hỏi, cho phép Clawd dùng **mạng riêng (Private)**.

## Bước 3: xong

Biểu tượng Clawd (ô vuông cam có mặt đồng hồ) xuất hiện trên thanh menu (Mac, góc
phải trên) hoặc khay hệ thống (Windows, góc phải dưới, có thể nằm trong mũi tên ^).
Chấm nhỏ trên biểu tượng cho biết tình trạng:

| Chấm | Nghĩa |
| --- | --- |
| xanh lá | Đã kết nối, đồng hồ đang hiện mức dùng |
| vàng | Đang tìm hoặc đang kết nối đồng hồ |
| đỏ | Có việc cần bạn làm: bấm vào biểu tượng để xem |

Lần mở đầu tiên, app mở **thiết lập ban đầu** (4 bước):

1. **Đồng hồ**: các đồng hồ app thấy trong mạng (mDNS); chọn một, hoặc nhập IP. Không thấy
   sau vài giây thì app chỉ cách cho đồng hồ vào WiFi (Bước 1 ở trên).
2. **Agent**: các agent có trên máy, đánh dấu sẵn. Chưa bấm **Tiếp** thì app chưa sửa gì.
3. **Usage**: đồng hồ hiện usage của Claude, của Codex CLI, hay không hiện.
4. **Xong**: những việc còn lại (ví dụ Codex: gõ `/hooks` trong Codex và tin cậy hook của Clawd).

Đóng giữa chừng cũng được: menu tray hiện **● Hoàn tất thiết lập…** cho tới khi xong, và tab
**App → Chạy lại…** mở lại bất cứ lúc nào. Người nâng cấp từ 0.1.x chỉ thấy hai bước mới
(Agent, Usage). Cài một agent mới sau này thì menu tray hỏi một lần **● Tìm thấy …: nối với
Clawd…**; bỏ qua là không hỏi lại. Ứng dụng tự mở cùng máy tính.

Bấm biểu tượng → **Cài đặt…** để mở cửa sổ cài đặt (mọi thứ ở một chỗ, không cần mở
trang web của đồng hồ):

- **Hiển thị**: độ sáng, xoay màn hình, màn hình chính, hoạt hình Clawd, múi giờ, chế độ đêm.
- **Usage**: chọn nguồn usage (Claude: mọi tài khoản hoặc một tài khoản; Codex CLI; không hiện)
  và kiểu màn hình usage, xem trước bằng mức dùng thật của bạn, có Clawd chuyển
  động như trên đồng hồ (bản không kèm firmware vẽ gần đúng, lấy hình Clawd từ chính đồng hồ
  của bạn và giữ trong `~/.config/claude-usage-monitor/clawd.pak`). Kiểu mà đồng hồ chưa có thì nút lưu thành **Cập nhật & áp dụng**: bấm thì app cài firmware đi kèm (khoảng 1 phút) rồi bật kiểu đó.
- **Wi‑Fi**: các mạng đồng hồ nhớ (tối đa 4, theo thứ tự ưu tiên), tìm mạng quanh đồng hồ,
  tên đồng hồ. Lưu Wi‑Fi thì đồng hồ khởi động lại.
- **Cập nhật**: cài firmware và gói hình (Clawd, desk, kiểu màn hình) mới nhất cho đồng hồ
  bằng một nút, không cần tải lại app. App tự kiểm tra bản mới lúc mở và mỗi 6 giờ. Khi có
  bản mới, menu tray hiện **● Có firmware mới**. Bấm **Kiểm tra bản mới** để hỏi ngay. Chỉ
  những gói khác với đồng hồ mới được gửi. Tab này cũng có sao lưu / khôi phục, khởi động
  lại và xoá cài đặt.
- **Agent**: mỗi agent một công tắc, kèm những gì agent đó báo được cho Clawd (suy nghĩ,
  công cụ, subagent, lỗi, cần bạn) và nhãn **Beta** cho agent chưa thử thật trên máy loại này.
  Bật là app thêm vài dòng "hook" vào cấu hình của agent (có sao lưu file cũ, file hỏng thì
  không đụng), tắt là gỡ đúng các dòng đó. **Ngắt kết nối tất cả** gỡ hết (nhất là trên Mac,
  nơi không có trình gỡ cài đặt). Agent chạy trong WSL chưa được hỗ trợ.
- **App**: mở cùng máy tính, chạy lại thiết lập ban đầu, chọn đồng hồ, mở nhật ký.

Đổi xong bấm **Lưu vào đồng hồ**: app gửi một lần, chỉ những gì đã đổi. Nếu đồng hồ có mật
khẩu trang cài đặt (in trên thẻ đi kèm), app hỏi một lần rồi nhớ trong Keychain (Mac) /
Credential Manager (Windows).

Trên Mac, bấm biểu tượng (chuột trái) mở một bảng nhỏ: Clawd chuyển động như trên đồng hồ,
hai thanh **Phiên** và **Tuần** kèm thời gian còn lại tới lúc đặt lại, nút bánh răng mở
**Cài đặt…** (hoặc ⌘,). Bấm ra ngoài hay Esc là đóng. Hình Clawd lấy từ chính đồng hồ của
bạn (một lần, giữ trong `~/.config/claude-usage-monitor/clawd.pak`); chưa lấy được thì bảng
vẽ biểu tượng app. Bấm chuột phải (hoặc Control + bấm) ra menu.

Menu (trên Windows: bấm chuột phải biểu tượng) chỉ có tình trạng (tên đồng hồ, % phiên và % tuần), **Cài đặt…** và **Thoát**. Tìm
lại đồng hồ, nhập địa chỉ, mở nhật ký (`~/.config/claude-usage-monitor/clawd-desktop.log`,
gửi cho người tặng đồng hồ nếu có lỗi) và trang web của đồng hồ đều nằm trong tab **App**
của cửa sổ cài đặt. Khi không thấy đồng hồ, menu hiện thêm **Nhập địa chỉ đồng hồ…** (dùng
được cả khi cửa sổ cài đặt không mở được): nhập IP mà đồng hồ hiện lúc khởi động (ví dụ
`192.168.1.48`).

Clawd **không quét mạng**. Nó tìm đồng hồ bằng mDNS (đồng hồ tự trả lời khi cùng mạng WiFi với máy
tính) và nhớ địa chỉ đã dùng. Khi đồng hồ không trả lời, Clawd chỉ thử lại đúng địa chỉ đó (mỗi
phút một lần, một máy); biểu tượng chuyển đỏ và nhắc nhập IP mới nếu đồng hồ đã đổi địa chỉ.
**Tìm lại** giữ địa chỉ cũ, thử lại ngay và để mDNS thay nó nếu đồng hồ ở mạng khác.

## Gặp vấn đề

| Hiện tượng | Cách xử lý |
| --- | --- |
| "Không thấy đồng hồ" | Đồng hồ và máy tính phải cùng một WiFi (không dùng mạng khách). Thử **Cài đặt… → App → Tìm lại**, hoặc **Nhập địa chỉ đồng hồ…** |
| Máy cắm dây LAN, đồng hồ ở WiFi công ty (hai dải IP khác nhau, vd `10.97.36.x` và `10.97.40.x`) | mDNS không đi qua router nên Clawd không tự thấy đồng hồ. Nhập một lần IP mà đồng hồ hiện lúc khởi động (**Nhập địa chỉ đồng hồ…**). Nhờ IT giữ cố định IP cho đồng hồ (DHCP reservation) để khỏi phải nhập lại khi nó đổi IP |
| Lúc được lúc mất, nhất là ở công ty, quán cà phê, khách sạn | Nhiều mạng công ty / mạng khách chặn các thiết bị nói chuyện với nhau (đồng hồ vẫn vào WiFi, vẫn hiện IP, nhưng máy tính không tới được). Ứng dụng không vượt qua được: dùng WiFi nhà, hoặc phát WiFi từ điện thoại rồi cho cả máy tính và đồng hồ vào đó (thêm mạng này trong trang cài đặt đồng hồ, mục WiFi) |
| "Đăng nhập Claude hết hạn" (khay hoặc cửa sổ Cài đặt), đồng hồ ghi "Claude login expired" | Đồng hồ vẫn kết nối, chỉ là đăng nhập Claude đã hết hạn hoặc chưa có. Mở Claude Code gửi một câu (nó tự gia hạn), hoặc mở Terminal (Mac) / PowerShell (Windows) gõ `claude login`. Ứng dụng tự nhận trong 1 phút |
| Đồng hồ hiện SETUP MODE | Đồng hồ không vào được WiFi (đổi mật khẩu WiFi, router mới...). Làm lại Bước 1 |
| Mac báo "Clawd bị hỏng, nên chuyển vào Thùng rác" | Chạy lệnh `xattr` ở mục Mac phía trên |

## Gỡ cài đặt

- **Mac**: **Cài đặt… → Agent → Ngắt kết nối tất cả**, rồi bấm biểu tượng → **Thoát** và kéo
  **Clawd** từ Applications vào Thùng rác. Hoặc chạy
  `/Applications/Clawd.app/Contents/MacOS/Clawd cleanup` (gỡ tự chạy và mọi hook / extension
  của Clawd trong các agent).
- **Windows**: **Cài đặt → Ứng dụng**, chọn **Clawd** → **Gỡ cài đặt**. Trình gỡ tự tắt
  ứng dụng, gỡ tự chạy và mọi hook / extension của Clawd trong các agent.

## Nhánh `python-0.1.x` (bản Python, sắp ngừng hỗ trợ)

Từ 0.2.0, Clawd là ứng dụng Rust + Tauri ở nhánh `main` (`desktop-rs/`). Nhánh này giữ bản
Python 0.1.x để vẫn phát hành được khi cần:

- **Hỗ trợ tới:** sửa lỗi bảo mật cho 0.1.x trong **3 tháng sau ngày phát hành 0.2.0**, hoặc sớm hơn
  nếu số lượt tải trên trang Releases cho thấy phần lớn người dùng đã lên 0.2.x. Sau đó nhánh chỉ
  còn để tham khảo.
- **Bản lùi (rollback):** một bản Python có số phiên bản cao hơn bản Rust đang chạy (ví dụ `0.2.9`).
  Bản Rust chỉ nhận bản cập nhật có `SHA256SUMS.minisig` ký bằng khoá phát hành, nên bản lùi phải được
  ký tay (xem `tools/migration/fork-release.md` mục 6 trên `main`).
- Tag `v0.1.x` đẩy từ nhánh này chạy `release.yml` của nhánh này (Python) và release đó thành
  **latest**. Sau khi phát một bản 0.1.x, kiểm tra lại `releases/latest` và nếu cần thì đặt lại bản
  Rust mới nhất làm latest: `gh release edit vX.Y.Z --latest`.

## Dành cho người phát triển

```sh
uv sync --python 3.12 --group dev
uv run --python 3.12 --group dev python -m pytest     # chạy test
make dev        # cửa sổ Cài đặt từ mã nguồn: sửa ui/ → trang tự tải lại (giữ tab), sửa .py → tự mở lại
make dev-tray   # tray từ mã nguồn, sửa .py → tự chạy lại (tắt Clawd.app đã cài trước: pkill -x Clawd)
uv run --python 3.12 python -m clawd_desktop          # chạy tray từ mã nguồn
uv run --python 3.12 python -m clawd_desktop run      # chạy không giao diện, log ra terminal
uv run --python 3.12 python -m clawd_desktop find     # liệt kê đồng hồ trong mạng
uv run --python 3.12 python -m clawd_desktop settings # mở cửa sổ cài đặt (đồng hồ đã nhớ)
python tools/bundle_firmware.py <firmware.bin> --src ../clawd-home/firmware   # kèm firmware…
python tools/build_preview.py --src ../clawd-home/firmware   # bộ vẽ màn hình đồng hồ cho cửa sổ Cài đặt (cần c++)
python tools/build_preview.py --src … --zip out.zip   # …đóng thành zip cho nguồn firmware (clawd-home tools/fetch_renderer.sh)
CLAWD_PRIVATE_FIRMWARE=1 sh packaging/build_mac.sh    # …vào bản build (chỉ cho chủ đồng hồ)
CLAWD_FEED_URL=https://… CLAWD_FEED_TOKEN=… sh packaging/build_mac.sh   # bản build biết nguồn firmware
uv run python tools/feed_e2e.py --feed http://127.0.0.1:8799 --token test-token …   # thử nguồn firmware (xem đầu file)
sh packaging/build_mac.sh                             # build Clawd.app + .dmg (trên Mac)
./packaging/build_windows.ps1                         # build bộ cài .exe (trên Windows)
```

Phát hành: tăng `__version__` trong `clawd_desktop/__init__.py` **và** `version` trong
`pyproject.toml` (hai nơi phải giống nhau và giống tag), rồi đẩy tag `vX.Y.Z` lên GitHub.
Workflow **Release** build `.dmg` + `.zip` (Apple silicon và Intel) và `-setup.exe`, tạo
`SHA256SUMS`, rồi phát hành một GitHub Release (tag có dấu `-`, ví dụ `v1.2.3-rc1`, là
prerelease: app không tự cập nhật lên). Hai secret `CLAWD_FEED_URL` và `CLAWD_FEED_TOKEN`
cho bản build biết nguồn firmware; thiếu thì app vẫn chạy nhưng không tự tìm được firmware mới.

Xem trước màn hình usage: bản public không kèm bộ vẽ. Mỗi bản phát hành trên nguồn firmware
có kèm bộ vẽ cho Windows và Mac (danh sách `renderers` trong manifest). App tải bộ vẽ đúng hệ
điều hành, giải nén vào `~/.config/claude-usage-monitor/renderer/` và kiểm tra hash từng file
mỗi lần mở cửa sổ Cài đặt. Nếu bản phát hành không có bộ vẽ hoặc đang offline, cửa sổ Cài đặt dùng
bản xem trước gần đúng như trước.

Ký và notarize bản mac (tuỳ chọn nhưng nên có): thêm các secret `MACOS_CERT_P12` (file
`.p12` của *Developer ID Application*, mã hoá base64), `MACOS_CERT_PASSWORD`,
`MACOS_SIGN_IDENTITY` (`Developer ID Application: Tên (TEAMID)`), `APPLE_ID`, `APPLE_TEAM_ID`,
`APPLE_APP_PASSWORD` (mật khẩu ứng dụng của Apple ID). Thiếu chúng thì build mac chỉ ký
ad-hoc và workflow in một cảnh báo.

### Tự cập nhật app

App hỏi `api.github.com/repos/vuduong97/usage-monitor/releases/latest` (công khai, không token)
sau khi mở 90 giây rồi mỗi 6 giờ; bản prerelease và draft bị bỏ qua. Có bản mới hơn thì app tải
nền file hợp với máy (`Clawd-X.Y.Z-mac-<arm64|x86_64>.zip` hoặc `Clawd-X.Y.Z-windows-setup.exe`),
so SHA-256 với `SHA256SUMS` của release, và trên mac kiểm tra chữ ký, bundle id, số phiên bản
và **Team ID trùng với app đang chạy**. Xong thì tray và tab Cập nhật hiện *Khởi động lại để
cập nhật*; chỉ khi người dùng bấm mới thay app (mac: script nền chờ app thoát, thay bundle,
mở lại; Windows: chạy bộ cài im lặng đè lên bản cũ). Không thay khi đang cập nhật firmware
cho đồng hồ. Mac chạy từ ổ đĩa `.dmg`, thư mục không ghi được, hoặc bản chỉ ký ad-hoc (không có Team ID để
đối chiếu) thì chỉ báo có bản mới. Giới hạn đã biết: trên Windows bộ cài chưa ký (Authenticode),
nên SHA-256 chỉ chứng minh file đúng với `SHA256SUMS` của cùng release, không chứng minh ai tạo ra nó.
Trạng thái nằm trong `~/.config/claude-usage-monitor/app-update/`; `auto_update = off`
trong file config tắt việc tự kiểm tra (nút *Kiểm tra bản mới* vẫn dùng được),
`update_repo = owner/name` đổi repo. Bản cài đầu tiên có tính năng này phải cài tay; từ đó trở đi
mới tự cập nhật.

Cấu trúc:

```
clawd_desktop/
  usage.py       đọc token Claude Code, hỏi mức dùng, gửi lên đồng hồ (từ daemon cũ)
  events.py      nhận hook của các agent (/hook/<agent>), quyết định Clawd làm gì (POST /api/state)
  agents/        mỗi agent một module: tìm trên máy, thêm/gỡ hook an toàn, đọc payload
  firstrun.py    thiết lập ban đầu: bước nào xong, agent nào đã hỏi
  codex_usage.py mức dùng Codex CLI, đọc từ log phiên của chính Codex (không gọi mạng)
  discovery.py   tìm đồng hồ: mDNS _clawd._tcp, địa chỉ đã nhớ, địa chỉ nhập tay (không quét mạng)
  engine.py      chạy tất cả trong một luồng nền, báo trạng thái cho tray
  tray.py        biểu tượng + menu tiếng Việt
  settings_app.py  cửa sổ cài đặt (pywebview, tiến trình riêng), trang ở ui/
  settings_api.py  những gì trang được gọi xuống Python (kiểm tra mọi giá trị)
  clockapi.py    API cài đặt của đồng hồ (/api/config, /update, digest auth)
  firmware.py, ota.py   firmware đi kèm app và cách cài (tạm dừng gửi qua pause.py)
  feed.py        nguồn firmware: hỏi bản mới, tải, giải mã trong RAM, so CRC gói với đồng hồ
  appupdate.py   tự cập nhật app: GitHub Releases, SHA-256, Team ID, thay app khi người dùng bấm
  hooks.py       hook Claude Code trong settings.json, và script clawd-event.sh dùng chung
  autostart.py   LaunchAgent (Mac) / HKCU Run (Windows)
integrations/    extension pi, plugin opencode (app chép vào agent) và lệnh `clawd`
packaging/       PyInstaller, DMG, Inno Setup
PROTOCOL.md      giao thức giữa máy tính và đồng hồ
```

Tuỳ chọn nâng cao nằm trong `~/.config/claude-usage-monitor/config` (dạng `khoá = giá trị`),
dùng chung với daemon cũ: `http_targets = 192.168.1.48` để chỉ định đồng hồ cố định,
`config_dirs = ~/.claude, ~/.claude-work` cho nhiều tài khoản, `events = off` để tắt phần
hoạt hình, `event_port = 47913`, `feed_channel = beta` để nhận bản thử trước.

Repo này **không chứa** firmware, font hay hình Clawd của đồng hồ (`tools/check_public.py`
chặn trong CI). Xem `NOTICE`. Firmware chỉ vào bản build khi chạy `tools/bundle_firmware.py`
(thư mục `clawd_desktop/firmware/` bị git bỏ qua); bản build như vậy chứa firmware riêng tư,
nên chỉ phát cho người dùng đồng hồ Clawd, đừng đính vào GitHub Release công khai. Cách
phát cho mọi người là **nguồn firmware** (`feed/` trong clawd-home). App tải firmware ở
dạng mã hoá, chỉ giải mã trong bộ nhớ rồi gửi thẳng lên đồng hồ, không ghi file nào ra đĩa.

### Phát style pack mới (người dùng không cài lại app)

Bản build chính thức biết nguồn firmware qua hai secret của repo này, `CLAWD_FEED_URL` và
`CLAWD_FEED_TOKEN` (đặt một lần: `gh secret set CLAWD_FEED_URL`, rồi
`gh secret set CLAWD_FEED_TOKEN < file-token`; không dán token vào lệnh hay chat). Thiếu
chúng thì log cửa sổ Cài đặt ghi `update feed: none` và thư viện kiểu chỉ có "Hiện tại".
Secret chỉ vào bản build mới: sau khi đặt, phát một bản app mới (tag `vX.Y.Z`).

Từ đó, ra pack mới chỉ làm ở clawd-home, không đụng repo này:

1. Viết `packs/<id>.js`, thêm id vào `ORDER` trong `firmware/tools/style/build.js`, rồi
   `node firmware/tools/style/build.js <fw-src>`.
2. Commit, tăng `CLAWD_FW_SEQ` trong `firmware/src/config.h` (không tự tăng; phải lớn hơn bản
   đã phát hành), chạy `tools/release.sh --notes "..."` (lên kênh **beta**).
3. Thử bằng `feed_channel = beta` trong file config của app, rồi `tools/release.sh --promote`
   để lên **stable**.

App hỏi nguồn mỗi 10 phút khi cửa sổ Cài đặt mở, tải pack (mã hoá, giải mã trong RAM). Đồng
hồ đã chạy cùng `styleApi` dùng pack mới ngay, không phải cài lại firmware. Tab Cập nhật vẫn
báo "có firmware mới" vì `seq` tăng.
