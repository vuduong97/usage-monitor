# Clawd

Ứng dụng nhỏ cho máy Mac và Windows: tự tìm đồng hồ Clawd trong mạng WiFi nhà bạn,
rồi hiện lên đồng hồ mức bạn đã dùng Claude (phiên 5 giờ và cả tuần). Khi bạn dùng
Claude Code, chú cua Clawd trên đồng hồ cũng nhảy theo: gõ phím khi Claude sửa code,
nhìn quanh khi Claude hỏi bạn, ngủ khi bạn rời máy.

Không cần nhập địa chỉ IP, không cần sửa file cấu hình.

## Cần có

- Đồng hồ Clawd (đã được cài sẵn phần mềm clawd).
- Máy Mac (macOS 11 trở lên) hoặc Windows 10/11, **cùng mạng WiFi với đồng hồ**.
- Claude Code đã đăng nhập trên máy đó (gõ `claude` một lần, đăng nhập tài khoản Claude Pro/Max).

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

Trong vòng 1–2 phút, đồng hồ hiện mức dùng Claude của bạn. Ứng dụng tự mở cùng máy tính.

Bấm biểu tượng → **Cài đặt…** để mở cửa sổ cài đặt (mọi thứ ở một chỗ, không cần mở
trang web của đồng hồ):

- **Hiển thị**: độ sáng, xoay màn hình, màn hình chính, hoạt hình Clawd, múi giờ, chế độ đêm.
- **Usage**: chọn kiểu màn hình usage ("Hiện tại" hoặc "Nhịp & dự báo"), xem trước bằng dữ
  liệu mẫu. Kiểu mà đồng hồ chưa có thì nút lưu thành **Cập nhật & áp dụng**: bấm thì app cài firmware đi kèm (khoảng 1 phút) rồi bật kiểu đó.
- **Wi‑Fi**: các mạng đồng hồ nhớ (tối đa 4, theo thứ tự ưu tiên), tìm mạng quanh đồng hồ,
  tên đồng hồ. Lưu Wi‑Fi thì đồng hồ khởi động lại.
- **Cập nhật**: cập nhật firmware đi kèm app, sao lưu / khôi phục, khởi động lại, xoá cài đặt.
- **App**: mở cùng máy tính, Clawd phản ứng khi dùng Claude Code (thêm vài dòng "hook"
  vào `~/.claude/settings.json`, có sao lưu file cũ, tắt đi là gỡ đúng các dòng đó), chọn
  đồng hồ, mở nhật ký.

Đổi xong bấm **Lưu vào đồng hồ**: app gửi một lần, chỉ những gì đã đổi. Nếu đồng hồ có mật
khẩu trang cài đặt (in trên thẻ đi kèm), app hỏi một lần rồi nhớ trong Keychain (Mac) /
Credential Manager (Windows).

Menu còn mấy lối tắt dùng được cả khi cửa sổ cài đặt không mở được:

- **Tìm lại đồng hồ**: dò lại từ đầu (sau khi đổi router, đổi WiFi...).
- **Nhập địa chỉ đồng hồ…**: nếu mạng nhà bạn chặn việc dò tìm, nhập IP mà đồng hồ hiện
  lúc khởi động (ví dụ `192.168.1.48`).
- **Mở trang cài đặt đồng hồ**: trang web của đồng hồ (vẫn dùng được như trước).
- **Mở nhật ký**: file ghi lại mọi việc ứng dụng làm (`~/.config/claude-usage-monitor/clawd-desktop.log`),
  gửi cho người tặng đồng hồ nếu có lỗi.

## Gặp vấn đề

| Hiện tượng | Cách xử lý |
| --- | --- |
| "Không thấy đồng hồ" | Đồng hồ và máy tính phải cùng một WiFi (không dùng mạng khách). Thử **Tìm lại đồng hồ**, hoặc **Nhập địa chỉ đồng hồ…** |
| Lúc được lúc mất, nhất là ở công ty, quán cà phê, khách sạn | Nhiều mạng công ty / mạng khách chặn các thiết bị nói chuyện với nhau (đồng hồ vẫn vào WiFi, vẫn hiện IP, nhưng máy tính không tới được). Ứng dụng không vượt qua được: dùng WiFi nhà, hoặc phát WiFi từ điện thoại rồi cho cả máy tính và đồng hồ vào đó (thêm mạng này trong trang cài đặt đồng hồ, mục WiFi) |
| "Chưa đăng nhập Claude" | Mở Terminal (Mac) hoặc PowerShell (Windows), gõ `claude`, đăng nhập. Ứng dụng tự nhận trong 1 phút |
| Đồng hồ hiện SETUP MODE | Đồng hồ không vào được WiFi (đổi mật khẩu WiFi, router mới...). Làm lại Bước 1 |
| Mac báo "Clawd bị hỏng, nên chuyển vào Thùng rác" | Chạy lệnh `xattr` ở mục Mac phía trên |

## Gỡ cài đặt

- **Mac**: bấm biểu tượng → **Thoát**. Mở Terminal, chạy
  `/Applications/Clawd.app/Contents/MacOS/Clawd cleanup` (gỡ tự chạy và hook Claude Code),
  rồi kéo **Clawd** từ Applications vào Thùng rác.
- **Windows**: **Cài đặt → Ứng dụng**, chọn **Clawd** → **Gỡ cài đặt**. Trình gỡ tự tắt
  ứng dụng, gỡ tự chạy và hook Claude Code.

## Dành cho người phát triển

```sh
uv sync --python 3.12 --group dev
uv run --python 3.12 --group dev python -m pytest     # chạy test
uv run --python 3.12 python -m clawd_desktop          # chạy tray từ mã nguồn
uv run --python 3.12 python -m clawd_desktop run      # chạy không giao diện, log ra terminal
uv run --python 3.12 python -m clawd_desktop find     # liệt kê đồng hồ trong mạng
uv run --python 3.12 python -m clawd_desktop settings # mở cửa sổ cài đặt (đồng hồ đã nhớ)
python tools/bundle_firmware.py <firmware.bin> --src ../clawd-home/firmware   # kèm firmware…
CLAWD_PRIVATE_FIRMWARE=1 sh packaging/build_mac.sh    # …vào bản build (chỉ cho chủ đồng hồ)
sh packaging/build_mac.sh                             # build Clawd.app + .dmg (trên Mac)
./packaging/build_windows.ps1                         # build bộ cài .exe (trên Windows)
```

Phát hành: đẩy tag `vX.Y.Z` lên GitHub, workflow **Release** build `.dmg` (Apple silicon và
Intel) và `.exe`, rồi đính vào một GitHub Release. Nhớ tăng `__version__` trong
`clawd_desktop/__init__.py`.

Cấu trúc:

```
clawd_desktop/
  usage.py       đọc token Claude Code, hỏi mức dùng, gửi lên đồng hồ (từ daemon cũ)
  events.py      nhận hook Claude Code / pi, quyết định Clawd làm gì (POST /api/state)
  discovery.py   tìm đồng hồ: mDNS _clawd._tcp, quét mạng /24, địa chỉ nhập tay
  engine.py      chạy tất cả trong một luồng nền, báo trạng thái cho tray
  tray.py        biểu tượng + menu tiếng Việt
  settings_app.py  cửa sổ cài đặt (pywebview, tiến trình riêng), trang ở ui/
  settings_api.py  những gì trang được gọi xuống Python (kiểm tra mọi giá trị)
  clockapi.py    API cài đặt của đồng hồ (/api/config, /update, digest auth)
  firmware.py, ota.py   firmware đi kèm app và cách cài (tạm dừng gửi qua pause.py)
  hooks.py       thêm/gỡ hook Claude Code trong settings.json
  autostart.py   LaunchAgent (Mac) / HKCU Run (Windows)
integrations/    extension pi và lệnh `clawd` (tuỳ chọn, cho người dùng nâng cao)
packaging/       PyInstaller, DMG, Inno Setup
PROTOCOL.md      giao thức giữa máy tính và đồng hồ
```

Tuỳ chọn nâng cao nằm trong `~/.config/claude-usage-monitor/config` (dạng `khoá = giá trị`),
dùng chung với daemon cũ: `http_targets = 192.168.1.48` để chỉ định đồng hồ cố định,
`config_dirs = ~/.claude, ~/.claude-work` cho nhiều tài khoản, `events = off` để tắt phần
hoạt hình, `event_port = 47913`.

Repo này **không chứa** firmware, font hay hình Clawd của đồng hồ (`tools/check_public.py`
chặn trong CI). Xem `NOTICE`. Firmware chỉ vào bản build khi chạy `tools/bundle_firmware.py`
(thư mục `clawd_desktop/firmware/` bị git bỏ qua); bản build như vậy chứa firmware riêng tư,
nên chỉ phát cho người dùng đồng hồ Clawd, đừng đính vào GitHub Release công khai.
