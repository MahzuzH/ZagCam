# PRD: ZagCam — Virtual Camera Ringan untuk Blur dan Ganti Background

Draf v0.1 · 13 November 2026 · Status: Draft untuk review · “ZagCam” hanyalah nama kerja sementara

## 1. Ringkasan

ZagCam adalah aplikasi desktop ringan yang mengambil video dari webcam fisik, memproses background-nya secara real-time (blur, ganti gambar, GIF, atau ganti video), lalu menampilkan hasilnya sebagai webcam virtual. Discord, Google Meet, Microsoft Teams, dan aplikasi lain hanya membaca kamera dari sistem operasi, jadi pengguna cukup memilih “ZagCam” sebagai kamera. Tidak perlu integrasi khusus per platform.

Pembeda utamanya: sangat ringan (RAM, CPU, dan ukuran installer kecil), ditulis dengan Rust, dan tidak memakai resource sama sekali ketika tidak ada aplikasi yang sedang memakai kameranya.

## 2. Latar Belakang dan Masalah

- Fitur blur dan background bawaan di Meet, Teams, dan Discord berbeda-beda, kualitasnya tidak seragam, dan pengaturannya harus diulang di tiap aplikasi.
- Alternatif yang ada (NVIDIA Broadcast, XSplit VCam, Camo, plugin background removal di OBS) umumnya berat, butuh GPU tertentu, berbayar, atau butuh setup yang rumit.
- Pengguna laptop kelas menengah atau lama sering mengalami kipas kencang dan lag ketika meeting sambil memakai efek background.

## 3. Tujuan dan Non-Tujuan

### Tujuan

1. Satu pengaturan background yang berlaku di semua aplikasi video call.
2. Overhead sangat kecil: RAM, CPU, dan ukuran installer jauh di bawah alternatif yang ada.
3. Kualitas tepi (rambut, bahu, tangan) cukup baik dan stabil antar frame, tanpa berkedip.
4. Setup sederhana: install, buka aplikasi meeting, pilih kamera “ZagCam”.
5. Privasi: seluruh pemrosesan berjalan lokal di perangkat pengguna.

### Non-Tujuan (untuk v1)

- Filter kecantikan, face tracking, atau efek AR.
- Fitur streaming, recording, atau scene composition ala OBS.
- Efek audio atau pengolahan mikrofon.
- Integrasi langsung (plugin/SDK) ke Discord, Meet, atau Teams.
- Versi mobile.

## 4. Target Pengguna

- **Pekerja remote dan pelajar** yang sering meeting di Meet atau Teams dan ingin background rapi tanpa membebani laptop.
- **Gamer dan streamer kecil** yang memakai Discord dan butuh kamera dengan background bersih, sementara CPU/GPU sudah terpakai game.
- **Pengguna laptop spesifikasi rendah** yang tidak kuat menjalankan aplikasi berat seperti NVIDIA Broadcast atau OBS.

## 5. User Stories

- Sebagai pekerja remote, saya ingin mengaktifkan blur background sekali saja, supaya berlaku di Meet, Teams, dan Discord tanpa mengatur ulang.
- Sebagai pengguna laptop lama, saya ingin efek background tidak membuat kipas berisik dan meeting tidak lag.
- Sebagai pengguna, saya ingin mengganti background dengan gambar, GIF, atau video favorit saya.
- Sebagai pengguna, saya ingin aplikasi berjalan di tray dan tidak memakai kamera atau CPU ketika tidak sedang meeting.
- Sebagai pengguna, saya ingin pindah antar preset (misalnya “Blur ringan”, “Kantor”) lewat satu klik atau hotkey.

## 6. Lingkup Fitur

### MVP (Windows)

- Capture dari webcam fisik dan memilih kamera sumber.
- Mode: tanpa efek, blur background (dengan tingkat blur yang bisa diatur), ganti dengan gambar.
- Output sebagai kamera virtual yang terdeteksi di Chrome, Edge, Discord, dan Teams.
- Tray app dengan toggle cepat dan jendela pengaturan sederhana.
- Buka kamera hanya saat kamera virtual sedang dipakai aplikasi lain.

### v1.0

- Ganti background dengan GIF dan video (loop).
- Preset dan hotkey global.
- Preview langsung di jendela pengaturan.
- Pengaturan kualitas (Hemat, Seimbang, Kualitas) yang mengatur frekuensi inferensi dan resolusi.
- Mirror/flip, serta penyesuaian dasar (brightness, kontras).
- Auto-start bersama sistem operasi (opsional).
- Installer bertanda tangan digital dan mekanisme update.

### Setelah v1.0

- Linux (v4l2loopback).
- macOS (CoreMediaIO Camera Extension).
- Dukungan Windows 10 (DirectShow filter), jika permintaan pasarnya cukup besar.
- Akselerasi GPU/NPU (DirectML, CoreML) sebagai opsi.
- Model matting dengan kualitas tepi lebih tinggi sebagai opsi “Kualitas tinggi”.

## 7. Persyaratan Fungsional

- **FR-1** Aplikasi mendeteksi semua webcam fisik dan memungkinkan pengguna memilih satu sebagai sumber.
- **FR-2** Aplikasi mendaftarkan satu kamera virtual yang muncul di daftar kamera aplikasi lain.
- **FR-3** Aplikasi memproses setiap frame lewat pipeline: capture → resize kecil → segmentasi → penghalusan mask → compositing → output.
- **FR-4** Pengguna dapat memilih mode: Off (pass-through), Blur, Gambar, GIF (v1.0), dan Video (v1.0).
- **FR-5** Intensitas blur dapat diatur.
- **FR-6** Mask dihaluskan antar frame (misalnya EMA) agar tepi tidak berkedip.
- **FR-7** Webcam fisik hanya dibuka ketika ada konsumen yang membuka kamera virtual, dan dilepas lagi sesaat setelah konsumen terakhir berhenti.
- **FR-8** Jika webcam fisik dicabut atau dipakai aplikasi lain, kamera virtual menampilkan frame pengganti (misalnya layar hitam atau gambar placeholder) dan pulih otomatis ketika webcam kembali tersedia.
- **FR-9** Pengaturan (sumber kamera, mode, gambar, preset) tersimpan dan dimuat ulang saat aplikasi dibuka.
- **FR-10** Aplikasi berjalan di system tray dan bisa ditutup penuh dari menu tray.
- **FR-11** Resolusi output minimal 720p pada 30 fps; 1080p opsional bila perangkat mampu.
- **FR-12** Background GIF diputar berulang (loop) mengikuti durasi tiap frame di file GIF. Jumlah frame dan resolusi yang disimpan di memori dibatasi (usulan awal 64 MB); GIF yang melebihi batas diperkecil atau ditolak dengan pesan yang jelas. File GIF dipilih dari penyimpanan lokal, tanpa unduhan dari internet.

## 8. Persyaratan Non-Fungsional (Target Awal)

Angka di bawah adalah target desain berdasarkan estimasi, dan harus divalidasi lewat pengukuran pada tahap prototipe.

- **Ukuran installer:** ≤ 30 MB untuk build CPU (ONNX Runtime + model kecil); ≤ 60 MB bila memakai akselerasi GPU dan model yang lebih besar.
- **RAM idle** (tray, kamera mati): ≤ 15 MB.
- **RAM aktif:** ≤ 100 MB (Private Working Set), termasuk proses provider kamera virtual.
- **CPU aktif:** ≤ 8% di laptop referensi (4 core, tanpa GPU diskrit) pada 720p/30 fps.
- **Latensi** capture ke output: ≤ 100 ms.
- **Frame rate:** stabil di 30 fps dan tidak turun di bawah 24 fps dalam kondisi normal.
- **Waktu siap:** kamera virtual menampilkan gambar dalam ≤ 1,5 detik setelah aplikasi meeting membukanya.
- **Stabilitas:** tidak crash selama sesi 4 jam; tidak ada memory leak yang terukur.
- **Idle CPU:** mendekati 0% saat tidak ada konsumen kamera.
- **Mode GIF:** tambahan RAM untuk cache frame GIF ≤ 64 MB di atas mode gambar statis (usulan awal), dan CPU/RAM-nya diukur terpisah dari mode blur.

## 9. Arsitektur Tingkat Tinggi

### Pipeline

Webcam asli → resize kecil (misalnya 256×144) → model segmentasi → upscale dan haluskan mask → composite (blur / gambar / video) → kamera virtual.

### Dua proses terpisah

- **Provider kamera virtual:** komponen kecil yang di-load atau diatur oleh OS. Tugasnya hanya menerima frame dan menyajikannya sebagai kamera. Dijaga seminimal mungkin supaya stabil.
- **Engine:** proses utama yang menangani capture, inferensi model, dan rendering. Mengirim frame ke provider lewat shared memory.

Pemisahan ini menjaga kode yang berjalan di dalam proses OS tetap kecil, dan memudahkan engine di-restart tanpa membuat kamera virtual hilang.

### Strategi performa

1. Inferensi di resolusi kecil, lalu upscale mask.
2. Jalankan model tiap 2 frame dan interpolasi mask di antaranya.
3. Penghalusan mask antar frame (EMA).
4. Blur dengan downscale → blur → upscale, atau di GPU bila tersedia.
5. Alokasikan buffer sekali di awal dan pakai ulang, tanpa alokasi per frame.
6. Buka kamera dan load model hanya saat ada konsumen (lihat FR-7).

## 10. Tech Stack (Usulan)

- **Bahasa:** Rust.
- **Capture webcam:** `nokhwa`.
- **Inferensi:** `ort` (ONNX Runtime) dengan DirectML di Windows dan CoreML di macOS. `tract` jadi kandidat bila ingin binary yang lebih kecil, dengan risiko dukungan operator yang terbatas.
- **Compositing dan blur:** CPU dengan SIMD (`fast_image_resize`) untuk versi paling ringan; `wgpu` sebagai opsi bila butuh GPU.
- **Decoding GIF:** crate `gif` (atau `image`), dengan frame disimpan pada resolusi asli GIF lalu di-scale saat compositing. Dibungkus di balik satu abstraksi (misalnya `BackgroundSource`) supaya decoder video bisa ditambahkan tanpa mengubah pipeline.
- **API Windows:** crate `windows`, dengan Media Foundation Virtual Camera (`MFCreateVirtualCamera`) untuk Windows 11.
- **UI:** tray app kecil dengan `egui` atau `iced`.
- **Model awal:** MediaPipe Selfie Segmentation (Apache 2.0, sangat kecil, jalan di CPU). Kandidat “kualitas tinggi”: MODNet. Robust Video Matting perlu dicek lisensinya (setahu saya GPL-3.0) sebelum dipakai di produk.
- **Build:** `opt-level = "s"`, LTO, `codegen-units = 1`, `panic = "abort"`, `strip = true`.

## 11. Dukungan Platform dan Tahapan

- **Fase 1, Windows 11:** target MVP lewat Media Foundation Virtual Camera.
- **Fase 2, Linux:** lewat v4l2loopback; teknis paling sederhana.
- **Fase 3, macOS 12.3+:** lewat CoreMediaIO Camera Extension (Swift/ObjC) yang dihubungkan ke engine Rust. Butuh Apple Developer account untuk signing dan notarization.
- **Windows 10:** membutuhkan DirectShow filter yang lebih rumit. Keputusan ditunda sampai ada data permintaan pengguna.

## 12. Matriks Kompatibilitas

Kamera virtual harus diuji di tiap kombinasi berikut sebelum rilis.

- **Google Meet:** Chrome, Edge, Firefox.
- **Microsoft Teams:** aplikasi desktop dan versi web. Di macOS, Teams dikenal lebih ketat soal kamera virtual, jadi perlu pengujian khusus.
- **Discord:** aplikasi desktop dan versi web.
- **Zoom** dan **OBS Studio** sebagai pembanding tambahan.
- Kasus khusus: beberapa aplikasi berjalan bersamaan memakai kamera virtual yang sama; aplikasi dibuka sebelum ZagCam berjalan; webcam dicabut saat meeting.

## 13. Alur Pengguna (UX)

1. Pengguna menginstal ZagCam dan memberi izin kamera bila diminta.
2. Aplikasi berjalan di tray. Jendela pengaturan menampilkan preview dan pilihan mode.
3. Pengguna memilih mode (misalnya Blur) dan tingkat intensitasnya.
4. Di Meet/Teams/Discord, pengguna memilih kamera “ZagCam”.
5. Efek langsung aktif. Pengguna dapat mengganti mode lewat tray atau hotkey tanpa menutup meeting.
6. Ketika meeting selesai, webcam dilepas dan penggunaan resource turun kembali ke idle.

Prinsip UX: sedikit pengaturan, default yang bagus, dan tidak ada dialog yang mengganggu saat meeting.

## 14. Privasi dan Keamanan

- Semua pemrosesan video berjalan lokal; tidak ada frame video yang dikirim keluar perangkat.
- Tidak ada akun atau login yang diwajibkan.
- Telemetri bersifat opsional (opt-in), dan tidak memuat konten video.
- Indikator yang jelas ketika webcam sedang aktif digunakan.
- Komponen yang berjalan di dalam proses OS dijaga minimal untuk memperkecil permukaan serangan.

## 15. Distribusi dan Rilis

- Installer dan binary wajib ditandatangani (code signing) agar tidak diblokir SmartScreen (Windows) atau Gatekeeper (macOS).
- Ada mekanisme update otomatis atau semi-otomatis.
- Siapkan penanganan false positive antivirus, karena aplikasi yang mendaftarkan driver atau kamera virtual sering terdeteksi mencurigakan.
- Uninstall harus bersih: kamera virtual dan komponen terkait ikut terhapus.

## 16. Metrik Keberhasilan

- Target performa pada bagian 8 tercapai di laptop referensi.
- Kamera virtual berhasil dikenali dan stabil di semua entri matriks kompatibilitas bagian 12.
- Crash-free session rate ≥ 99% pada beta.
- Dalam uji pengguna, ≥ 80% peserta dapat menyelesaikan setup dan memakai blur di meeting tanpa bantuan.
- Rasio pengguna yang menyalakan efek lebih dari 3 sesi meeting (retensi awal) menjadi indikator nilai produk.

## 17. Risiko dan Mitigasi

- **Kamera virtual rumit dan rawan kompatibilitas.** Mitigasi: validasi pipeline lebih dulu memakai OBS Virtual Camera atau v4l2loopback sebelum menulis provider sendiri; uji matriks aplikasi sejak awal.
- **Kualitas segmentasi** (rambut, tangan, objek di depan wajah) kurang memuaskan di model kecil. Mitigasi: sediakan model “Kualitas tinggi” sebagai opsi, dan penghalusan temporal.
- **Performa buruk di perangkat lama.** Mitigasi: mode Hemat (inferensi lebih jarang, resolusi lebih kecil), dan fallback otomatis bila CPU tinggi.
- **RAM membengkak karena background GIF.** Satu frame 720p RGBA sekitar 3,7 MB, jadi GIF panjang yang di-decode penuh ke memori bisa memakan ratusan MB. Mitigasi: batasi jumlah frame dan resolusi, simpan frame pada resolusi asli GIF, dan perkecil atau tolak GIF yang melebihi batas.
- **Aplikasi pesaing dan fitur bawaan** Meet/Teams/Discord sudah mencakup kebutuhan dasar. Mitigasi: fokus pada nilai jual yang jelas (paling ringan, satu pengaturan untuk semua aplikasi, ganti background video).
- **Lisensi model dan dependensi.** Mitigasi: audit lisensi sebelum dipakai; hindari model GPL bila produk tidak open source.
- **Signing, notarization, dan false positive antivirus.** Mitigasi: anggarkan sertifikat code signing sejak awal, dan rencanakan proses rilis yang konsisten.
- **Perbedaan perilaku antar OS,** terutama macOS. Mitigasi: rilis bertahap per platform dan mulai dari Windows.

## 18. Milestone

Estimasi durasi diisi setelah M0 selesai, karena kecepatan eksplorasi API kamera virtual belum bisa diperkirakan.

1. **M0 – Prototipe capture:** webcam tampil di jendela lewat `nokhwa`, ukur baseline CPU/RAM.
2. **M1 – Blur background:** integrasi `ort` + MediaPipe, mask dihaluskan, output ke OBS Virtual Camera untuk validasi.
3. **M2 – Kamera virtual sendiri (Windows 11):** provider + engine dengan shared memory, terdeteksi di Chrome, Discord, dan Teams.
4. **M3 – UI dan fitur MVP:** tray app, mode blur dan gambar, penyimpanan pengaturan, buka kamera hanya saat dibutuhkan.
5. **M4 – Optimasi dan beta tertutup:** tuning performa, uji matriks kompatibilitas, perbaikan stabilitas.
6. **M5 – Rilis v1.0 (Windows):** ganti background video, preset dan hotkey, installer bertanda tangan, mekanisme update.
7. **M6 – Linux**, lalu **M7 – macOS**.

## 19. Pertanyaan Terbuka

- Nama produk final dan identitas visualnya?
- Model bisnis: gratis, open source, freemium, atau berbayar?
- Apakah dukungan Windows 10 masuk ke v1.0, atau ditunda?
- Akselerasi GPU aktif secara default, atau hanya opsi lanjutan?
- Model mana yang jadi default: MediaPipe (paling ringan) atau MODNet (tepi lebih bagus)?
- Apakah ganti background dengan video benar-benar masuk v1.0, atau digeser agar MVP lebih cepat rilis?
- Apakah GIF dirilis lebih awal dari video (misalnya di MVP), karena decoding-nya jauh lebih sederhana?
- Bahasa UI awal: Indonesia, Inggris, atau keduanya?
