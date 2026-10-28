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
| Windows | `Clawd-x.y.z-windows-setup.exe` |

Mac chip Intel: bản cuối là **0.2.0** (`Clawd-0.2.0-mac-x86_64.dmg` ở trang Releases của 0.2.0); từ
0.2.1 chỉ còn bản cho chip Apple. Không biết Mac của mình chip gì? Bấm  → **Giới thiệu về máy
Mac**: dòng *Chip* ghi "Apple M..." là chip Apple, dòng *Bộ xử lý* ghi "Intel" là chip Intel.

### Mac

1. Mở file `.dmg`, kéo **Clawd** vào thư mục **Applications**.
2. Mở **Applications**, nhấp đúp **Clawd**. Ứng dụng được ký Developer ID và đã qua kiểm tra
   (notarize) của Apple, nên macOS chỉ hỏi *bạn có chắc muốn mở ứng dụng tải từ Internet* một
   lần: bấm **Mở**.
3. Nếu macOS hỏi *cho phép Clawd tìm thiết bị trong mạng cục bộ*, bấm **Cho phép**.
   Nếu hỏi quyền đọc **Chuỗi khoá** (Keychain) cho mục "Claude Code-credentials", bấm
   **Luôn cho phép**: đó là chỗ Claude Code cất thông tin đăng nhập, Clawd chỉ đọc để
   hỏi Anthropic mức dùng của bạn.

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

**Cập nhật ứng dụng:** app tự tải bản mới ở chế độ nền (kiểm tra lúc mở và mỗi 6 giờ). Tải xong,
menu hiện **● Khởi động lại để cập nhật**; app chỉ được thay khi bạn bấm vào đó. Cấu hình, đồng hồ
đã nhớ và hook của các agent được giữ nguyên. Sau lần cập nhật lên 0.2 (bản viết lại bằng Rust),
macOS có thể hỏi lại **một lần** quyền Chuỗi khoá và quyền mạng cục bộ: bấm **Luôn cho phép** /
**Cho phép** như lúc mới cài.

Bấm biểu tượng → **Cài đặt…** để mở cửa sổ cài đặt (mọi thứ ở một chỗ, không cần mở
trang web của đồng hồ):

- **Hiển thị**: độ sáng, xoay màn hình, màn hình chính, hoạt hình Clawd, múi giờ, chế độ đêm.
  Mục **Nâng cao: Animation theo skill** cho chọn animation và chữ riêng khi bạn gọi một skill
  (ví dụ `/ak-cook` → wizard "Cooking"), có nút ▶ phát thử trên đồng hồ, xuất/nhập. Chạy với
  Claude Code và pi; câu hỏi, lỗi, nghĩ lâu, chạy test, debug và đọc/tìm file vẫn hiện như cũ.
  Luật nằm trên máy (`~/.config/claude-usage-monitor/animation-rules.json`), lưu ngay, không
  gửi xuống đồng hồ; chỉ tên skill rời khỏi hook, không bao giờ nội dung prompt.
- **Usage**: chọn nguồn usage (Claude: mọi tài khoản hoặc một tài khoản; Codex CLI; không hiện)
  và kiểu màn hình usage, xem trước bằng mức dùng thật của bạn, có Clawd chuyển
  động như trên đồng hồ (vẽ bằng chính mã vẽ của firmware đi kèm app). Kiểu màn hình mới có thể
  tới giữa hai bản app: app tự tải gói đã ký về khi mở cửa sổ Cài đặt. Kiểu mà đồng hồ chưa có thì nút lưu thành **Cập nhật & áp dụng**: bấm thì app cài firmware đi kèm (khoảng 1 phút) rồi bật kiểu đó.
- **Wi‑Fi**: các mạng đồng hồ nhớ (tối đa 4, theo thứ tự ưu tiên), tìm mạng quanh đồng hồ,
  tên đồng hồ. Lưu Wi‑Fi thì đồng hồ khởi động lại.
- **Cập nhật**: cài firmware và gói hình (Clawd, desk) đi kèm app cho đồng hồ bằng một nút.
  Firmware chỉ đổi theo bản app: app mới mang firmware mới. Khi đồng hồ thiếu firmware hay gói
  hình của app, menu tray hiện **● Có firmware mới**. **Kiểm tra bản mới** hỏi ngay xem có kiểu
  màn hình usage mới không. Chỉ những gói khác với đồng hồ mới được gửi. Tab này cũng có sao lưu / khôi phục, khởi động
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
| Mac báo "Clawd bị hỏng, nên chuyển vào Thùng rác" | File tải về bị lỗi. Xoá Clawd khỏi Applications, tải lại `.dmg` từ trang Releases |

## Gỡ cài đặt

- **Mac**: **Cài đặt… → Agent → Ngắt kết nối tất cả**, rồi bấm biểu tượng → **Thoát** và kéo
  **Clawd** từ Applications vào Thùng rác. Hoặc chạy
  `/Applications/Clawd.app/Contents/MacOS/Clawd cleanup` (gỡ tự chạy và mọi hook / extension
  của Clawd trong các agent).
- **Windows**: **Cài đặt → Ứng dụng**, chọn **Clawd** → **Gỡ cài đặt**. Trình gỡ tự tắt
  ứng dụng, gỡ tự chạy và mọi hook / extension của Clawd trong các agent.

## Dành cho người phát triển

Từ 0.2.0 ứng dụng là Rust + Tauri v2; Cargo workspace nằm ở gốc repo. Bản Python 0.1.x không còn trên `main`:
mã, test và cách build của nó nằm ở nhánh `python-0.1.x` (và các tag `v0.1.x`).

```sh
cargo test --workspace                                   # chạy test
cargo clippy --workspace --all-targets -- -D warnings    # lint (CI chặn mọi cảnh báo)
cargo run -p clawd-app                                   # tray từ mã nguồn (tắt Clawd đã cài trước: pkill -x Clawd)
cargo run -p clawd-app -- run                            # chạy không giao diện, log ra terminal
cargo run -p clawd-app -- find                           # liệt kê đồng hồ trong mạng
cargo run -p clawd-app -- settings                       # mở cửa sổ cài đặt (trang ở app/ui/)
sh tools/build_clock.sh                               # firmware (clock/, cần PlatformIO) + bộ vẽ màn hình -> build/
sh packaging/build_mac.sh                             # build Clawd.app + .zip + .dmg (trên Mac), kèm build/firmware + build/renderer
./packaging/build_windows.ps1                         # build bộ cài .exe (trên Windows, cần Inno Setup 6)
```

Phát hành: đặt `version` trong `Cargo.toml` (`[workspace.package]`) **và**
`app/tauri.conf.json` (hai nơi phải giống nhau và giống tag; `cargo update --workspace`
để `Cargo.lock` theo), viết `packaging/release-notes/X.Y.Z.md`, rồi đẩy tag `vX.Y.Z`. Workflow
**Release** (`.github/workflows/release.yml`) chạy test, chờ chủ repo duyệt environment `release`,
build `.dmg` + `.zip` (Apple silicon, ký Developer ID và notarize) và `-setup.exe`, tạo
`SHA256SUMS` và chữ ký `SHA256SUMS.minisig`, rồi tạo một **bản nháp** (draft). Bản nháp chưa tới
tay ai: kiểm tra file, thử cập nhật (`tools/migration/local-release.md`), rồi phát hành bằng
`gh release edit vX.Y.Z --draft=false --latest` và kiểm `gh api repos/vuduong97/usage-monitor/releases/latest --jq .tag_name`
ra đúng tag thì mọi app mới nhận. Tag có dấu `-` (ví dụ `v1.2.3-rc.1`) là prerelease: app không tự cập nhật lên. Chi tiết
secret, khoá minisign và các cổng kiểm tra: `packaging/README.md`.

Firmware của đồng hồ nằm ngay trong repo này (`clock/`, trước đây là repo clawd-home). Mỗi bản
phát hành build firmware từ đúng tag (job `firmware` của Release) và đóng vào app cùng
`clawd.pak`, `desk.pak`, các style pack và bộ vẽ màn hình (`Contents/Resources/{firmware,renderer}`
trên Mac, cạnh `Clawd.exe` trên Windows). Đổi mã firmware (`clock/firmware/src`) thì phải tăng
`CLAWD_FW_SEQ` trong `clock/firmware/src/config.h`: CI chặn nếu quên, vì app chỉ cài firmware
lên đồng hồ đang chạy số nhỏ hơn.

Ký và notarize bản mac là **bắt buộc** khi phát hành: thiếu các secret `MACOS_CERT_P12` (file
`.p12` của *Developer ID Application*, mã hoá base64), `MACOS_CERT_PASSWORD`,
`MACOS_SIGN_IDENTITY` (`Developer ID Application: Tên (TEAMID)`), `APPLE_ID`, `APPLE_TEAM_ID`,
`APPLE_APP_PASSWORD` (mật khẩu ứng dụng của Apple ID) thì workflow dừng. Team ID phải trùng
biến `EXPECTED_TEAM_ID`, nếu không app đang chạy sẽ từ chối bản cập nhật.

### Tự cập nhật app

App hỏi `api.github.com/repos/vuduong97/usage-monitor/releases/latest` (công khai, không token)
sau khi mở 90 giây rồi mỗi 6 giờ; bản prerelease và draft bị bỏ qua. Có bản mới hơn thì app tải
nền file hợp với máy (`Clawd-X.Y.Z-mac-arm64.zip` hoặc `Clawd-X.Y.Z-windows-setup.exe`),
kiểm chữ ký minisign của `SHA256SUMS` (`SHA256SUMS.minisig`, khoá công khai nằm sẵn trong app; từ
0.2.0), so SHA-256 với `SHA256SUMS`, và trên mac kiểm tra chữ ký, bundle id, số phiên bản
và **Team ID trùng với app đang chạy**. Xong thì tray và tab Cập nhật hiện *Khởi động lại để
cập nhật*; chỉ khi người dùng bấm mới thay app (mac: script nền chờ app thoát, thay bundle,
mở lại; Windows: chạy bộ cài im lặng đè lên bản cũ). Không thay khi đang cập nhật firmware
cho đồng hồ. Mac chạy từ ổ đĩa `.dmg`, thư mục không ghi được, hoặc bản chỉ ký ad-hoc (không có Team ID để
đối chiếu) thì chỉ báo có bản mới. Giới hạn đã biết: trên Windows bộ cài chưa ký (Authenticode),
nên SmartScreen vẫn cảnh báo lúc cài tay; bản tự cập nhật được bảo vệ bằng chữ ký minisign.
Trạng thái nằm trong `~/.config/claude-usage-monitor/app-update/`; `auto_update = off`
trong file config tắt việc tự kiểm tra (nút *Kiểm tra bản mới* vẫn dùng được),
`update_repo = owner/name` đổi repo. Bản cài đầu tiên có tính năng này phải cài tay; từ đó trở đi
mới tự cập nhật.

Cấu trúc:

```
Cargo.toml           workspace (version của app ở [workspace.package]) và Cargo.lock
app/                 ứng dụng Tauri (binary Clawd / Clawd.exe): tray, engine, cửa sổ cài đặt (ui/), bảng nhỏ trên mac
crates/
  clawd-contracts    kiểu và hằng số dùng chung (đường dẫn, tên, cổng 47913, khoá minisign)
  clawd-config       file config, thư mục Claude, nguồn token OAuth (không gọi mạng)
  clawd-store        prefs, log, Keychain/Credential Manager, tự mở cùng máy (LaunchAgent / HKCU Run)
  clawd-events       sự kiện của agent, quyết định Clawd làm gì, server hook trên loopback
  clawd-agents       mỗi agent một adapter: tìm trên máy, thêm/gỡ hook an toàn, đọc payload
  clawd-usage        mức dùng Claude / Codex, gửi lên đồng hồ
  clawd-clock        API cài đặt của đồng hồ, tìm đồng hồ (mDNS, địa chỉ đã nhớ, nhập tay)
  clawd-firmware     firmware đi kèm, style pack từ xa (packs-api<N>), OTA, xem trước màn hình
  clawd-appupdate    tự cập nhật app: GitHub Releases, minisign, SHA-256, Team ID
  clawd-testkit      đồ giả và fixture dùng chung cho test
tests-e2e/           test xuyên crate
fixtures/            golden fixture ghi từ bản Python 0.1.x (đóng băng: sửa Rust, không sửa fixture)
packaging/           build mac (Tauri bundler, ký, notarize) và Windows (Inno Setup), kiểm tra version
integrations/        extension pi, plugin opencode (app chép vào agent) và lệnh `clawd`
clock/               firmware đồng hồ (PlatformIO, ESP8266), style pack, công cụ nạp/khôi phục, tài liệu phần cứng
tools/               build_clock / bundle_firmware / build_preview (firmware vào build/), publish_packs, check_fw_seq, perf, migration
PROTOCOL.md          giao thức giữa máy tính và đồng hồ
```

Tuỳ chọn nâng cao nằm trong `~/.config/claude-usage-monitor/config` (dạng `khoá = giá trị`),
dùng chung với daemon cũ: `http_targets = 192.168.1.48` để chỉ định đồng hồ cố định,
`config_dirs = ~/.claude, ~/.claude-work` cho nhiều tài khoản, `events = off` để tắt phần
hoạt hình, `event_port = 47913`, `packs_url = off` để không tải kiểu màn hình mới.

Repo này chứa firmware, font và gói hình Clawd của đồng hồ (`clock/`). Xem `NOTICE` về bản
quyền của hình Clawd và font.

### Phát style pack mới (người dùng không cài lại app)

Kiểu màn hình usage mới có thể tới tay người dùng giữa hai bản app, nếu firmware đang đi kèm
app đã vẽ được chúng (cùng `STYLE_API`). Firmware thì không: nó chỉ đổi theo bản app.

1. Viết `clock/firmware/tools/style/packs/<id>.js`, thêm id vào `ORDER` trong
   `clock/firmware/tools/style/build.js`, rồi build lại `clock/firmware/styles/` (cách chạy:
   `clock/docs/STYLE_PACK.md`). Không đổi mã firmware, không tăng `CLAWD_FW_SEQ`.
2. Commit lên `main`, rồi chạy workflow **Style packs** (`.github/workflows/packs.yml`, chủ repo
   duyệt environment `release`). Hoặc chạy tay: `sh tools/publish_packs.sh --publish`
   (`MINISIGN_KEY_FILE` là khoá bí mật minisign của app, `MINISIGN_PUBKEY` là khoá công khai).

Script ký `index.json` bằng đúng khoá minisign ký bản app, rồi đưa `index.json`,
`index.json.minisig` và các `<id>.pak` lên release cố định `packs-api<N>` (tạo với
`--latest=false`; script dừng báo lỗi nếu `releases/latest` không còn là tag `v*`, vì app 0.1.x
và 0.2.x tự cập nhật từ đó). `rev` của gói là thời điểm commit cuối của
`clock/firmware/styles` và phải tăng.

App 0.3.0 trở lên tải `https://github.com/vuduong97/usage-monitor/releases/download/packs-api<N>/index.json`
khi mở cửa sổ Cài đặt (tối đa mỗi 10 phút), kiểm chữ ký, kiểm `api` trùng `styleApi` của
firmware đi kèm, kiểm SHA-256 và header từng gói, giữ gói trong
`~/.config/claude-usage-monitor/style-packs/`, và chỉ dùng chúng khi `rev` lớn hơn gói đi kèm
app. Sai ở bất kỳ bước nào thì app giữ gói đi kèm. Đổi `STYLE_API` là đổi firmware: phải ra bản
app mới (release `packs-api<N+1>` mới sẽ được tạo khi publish lần đầu cho API đó).

App 0.2.x và cũ hơn vẫn đọc nguồn firmware cũ (Cloudflare, `clock/feed/`), giữ nguyên cho tới
khi phần lớn người dùng lên 0.3.0.
