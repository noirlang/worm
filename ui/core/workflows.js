function L(tr, en) {
  return { tr, en };
}

export function localText(value, language = "tr") {
  if (value && typeof value === "object" && "tr" in value) {
    return value[language] || value.tr;
  }
  return value;
}

export const toolCards = {
  windows: [
    {
      id: "windows-remote-disk",
      title: L("Uzak Disk İmajı", "Remote Disk Image"),
      desc: L("Agent ile PhysicalDrive imajı alın.", "Acquire a PhysicalDrive image through the agent."),
      icon: "disk",
      accent: "var(--text)",
      badge: "Agent + raw stream"
    },
    {
      id: "windows-local-disk",
      title: L("Yerel Disk İmajı", "Local Disk Image"),
      desc: L("Bu makinedeki diskten ham imaj alın.", "Acquire a raw image from this machine."),
      icon: "windows",
      accent: "var(--text)",
      badge: "PhysicalDrive"
    },
    {
      id: "windows-remote-ram",
      title: L("Uzak RAM", "Remote RAM"),
      desc: L("WinPMEM ile RAM dump alın.", "Acquire a RAM dump with WinPMEM."),
      icon: "ram",
      accent: "var(--text)",
      badge: "WinPMEM remote"
    },
    {
      id: "windows-local-ram",
      title: L("Yerel RAM", "Local RAM"),
      desc: L("WinPMEM ile yerel RAM alın.", "Acquire local RAM with WinPMEM."),
      icon: "chip",
      accent: "var(--text)",
      badge: L("Yönetici gerekli", "Admin required")
    },
    {
      id: "windows-ssh-disk",
      title: L("SSH/WinRM ile Uzak Disk (Agent'sız)", "SSH/WinRM Remote Disk (Agentless)"),
      desc: L("Windows OpenSSH veya WinRM bağlantısıyla agent kurmadan PhysicalDrive imajı alın.", "Acquire PhysicalDrive image from remote Windows via OpenSSH/WinRM without an agent."),
      icon: "key",
      accent: "var(--text)",
      badge: "SSH · PhysicalDrive"
    },
    {
      id: "windows-ssh-ram",
      title: L("SSH/WinRM ile Uzak RAM (Agent'sız)", "SSH/WinRM Remote RAM (Agentless)"),
      desc: L("Windows OpenSSH üzerinden WinPMEM ile agent kurmadan RAM dökümü alın.", "Acquire remote Windows RAM via OpenSSH with WinPMEM without an agent."),
      icon: "ram",
      accent: "var(--text)",
      badge: "SSH · WinPMEM pipe"
    }
  ],
  linux: [
    {
      id: "linux-remote-disk",
      title: L("Uzak Disk İmajı", "Remote Disk Image"),
      desc: L("Agent ile /dev disk imajı alın.", "Acquire a /dev disk image through the agent."),
      icon: "disk",
      accent: "var(--text)",
      badge: "Agent + /dev"
    },
    {
      id: "linux-local-disk",
      title: L("Yerel Disk İmajı", "Local Disk Image"),
      desc: L("Root ile yerel disk imajı alın.", "Acquire a local disk image as root."),
      icon: "linux",
      accent: "var(--text)",
      badge: "BLKGETSIZE64"
    },
    {
      id: "linux-remote-ram",
      title: L("Uzak RAM", "Remote RAM"),
      desc: L("AVML ile RAM dump alın.", "Acquire a RAM dump with AVML."),
      icon: "ram",
      accent: "var(--text)",
      badge: "AVML remote"
    },
    {
      id: "linux-local-ram",
      title: L("Yerel RAM", "Local RAM"),
      desc: L("AVML ile yerel RAM alın.", "Acquire local RAM with AVML."),
      icon: "chip",
      accent: "var(--text)",
      badge: L("Root gerekli", "Root required")
    },
    {
      id: "linux-ssh-disk",
      title: L("SSH ile Uzak Disk (Agent'sız)", "SSH Remote Disk (Agentless)"),
      desc: L("Hedef Linux sisteme SSH bağlantısıyla agent kurmadan disk imajı alın.", "Acquire a disk image from a remote Linux host via SSH without any agent."),
      icon: "key",
      accent: "var(--text)",
      badge: "SSH · dd pipe"
    },
    {
      id: "linux-ssh-ram",
      title: L("SSH ile Uzak RAM (Agent'sız)", "SSH Remote RAM (Agentless)"),
      desc: L("Hedef Linux sisteme SSH ile bağlanıp AVML veya /proc/kcore ile RAM dökümü alın.", "Dump remote Linux RAM via SSH using AVML or /proc/kcore without an agent."),
      icon: "ram",
      accent: "var(--text)",
      badge: "SSH · AVML pipe"
    }
  ]
};

export const workflows = {
  "windows-remote-disk": {
    platform: "Windows",
    icon: "windows",
    title: L("Uzak Windows Sunucu Bağlantısı", "Remote Windows Server Connection"),
    desc: L("Bağlanın, disk seçin, imaj alın.", "Connect, select a disk, acquire an image."),
    mode: "remote-disk",
    output: "/home/raodrin/Amele/Ciktilar",
    diskLabel: L("Disk seçilmedi", "No disk selected")
  },
  "linux-remote-disk": {
    platform: "Linux",
    icon: "linux",
    title: L("Uzak Linux Disk Bağlantısı", "Remote Linux Disk Connection"),
    desc: L("Bağlanın, /dev disk seçin, imaj alın.", "Connect, select a /dev disk, acquire an image."),
    mode: "remote-disk",
    output: "/home/raodrin/Amele/Ciktilar",
    diskLabel: L("Disk seçilmedi", "No disk selected")
  },
  "windows-local-disk": {
    platform: "Windows",
    icon: "windows",
    title: L("Windows Yerel Disk İmajı", "Windows Local Disk Image"),
    desc: L("PhysicalDrive seçin ve imaj alın.", "Select a PhysicalDrive and acquire an image."),
    mode: "local-disk",
    output: "C:\\Amele\\Ciktilar",
    diskLabel: L("Disk seçilmedi", "No disk selected")
  },
  "linux-local-disk": {
    platform: "Linux",
    icon: "linux",
    title: L("Linux Yerel Disk İmajı", "Linux Local Disk Image"),
    desc: L("Blok cihaz seçin ve imaj alın.", "Select a block device and acquire an image."),
    mode: "local-disk",
    output: "/home/raodrin/Amele/Ciktilar",
    diskLabel: L("Disk seçilmedi", "No disk selected")
  },
  "windows-remote-ram": {
    platform: "Windows",
    icon: "ram",
    title: L("Windows Uzak RAM Edinimi", "Windows Remote RAM Acquisition"),
    desc: L("WinPMEM kontrolü ve RAM dump indirme.", "Check WinPMEM and download the RAM dump."),
    mode: "remote-ram",
    output: "memory_dump.raw",
    diskLabel: "WinPMEM"
  },
  "linux-remote-ram": {
    platform: "Linux",
    icon: "ram",
    title: L("Linux Uzak RAM Edinimi", "Linux Remote RAM Acquisition"),
    desc: L("AVML kontrolü ve RAM dump indirme.", "Check AVML and download the RAM dump."),
    mode: "remote-ram",
    output: "memory_dump_linux.raw",
    diskLabel: "AVML"
  },
  "windows-local-ram": {
    platform: "Windows",
    icon: "chip",
    title: L("Windows Yerel RAM Edinimi", "Windows Local RAM Acquisition"),
    desc: L("WinPMEM kontrolü ve yerel RAM imajı.", "Check WinPMEM and acquire local RAM."),
    mode: "local-ram",
    output: "memory_dump_local.raw"
  },
  "linux-local-ram": {
    platform: "Linux",
    icon: "chip",
    title: L("Linux Yerel RAM Edinimi", "Linux Local RAM Acquisition"),
    desc: L("AVML kontrolü ve root ile RAM imajı.", "Check AVML and acquire RAM as root."),
    mode: "local-ram",
    output: "linux_memory_dump.raw"
  },
  "linux-ssh-disk": {
    platform: "Linux",
    icon: "key",
    title: L("SSH ile Agent'sız Linux Disk", "Agentless Linux Disk via SSH"),
    desc: L("Hedef IP, port, kullanıcı adı ve disk seçin; dd pipe ile imaj alınır.", "Enter target IP, port, username and disk; image is acquired via dd pipe."),
    mode: "ssh-disk",
    output: "/home/raodrin/Amele/Ciktilar",
    diskLabel: L("Disk seçilmedi", "No disk selected")
  },
  "linux-ssh-ram": {
    platform: "Linux",
    icon: "ram",
    title: L("SSH ile Agent'sız Linux RAM", "Agentless Linux RAM via SSH"),
    desc: L("Hedef IP, port ve kullanıcı adıyla SSH bağlan; AVML pipe ile RAM alınır.", "SSH into the target with IP, port and username; RAM is dumped via AVML pipe."),
    mode: "ssh-ram",
    output: "/home/raodrin/Amele/Ciktilar"
  },
  "windows-ssh-disk": {
    platform: "Windows",
    icon: "key",
    title: L("SSH / WinRM ile Agent'sız Windows Disk", "Agentless Windows Disk via SSH / WinRM"),
    desc: L("Hedef IP, port, kullanıcı adı ve disk seçin; PhysicalDrive imajı alınır.", "Enter target IP, port, username and disk; PhysicalDrive image is acquired."),
    mode: "ssh-disk",
    output: "C:\\Amele\\Ciktilar",
    diskLabel: L("Disk seçilmedi", "No disk selected")
  },
  "windows-ssh-ram": {
    platform: "Windows",
    icon: "ram",
    title: L("SSH / WinRM ile Agent'sız Windows RAM", "Agentless Windows RAM via SSH / WinRM"),
    desc: L("Hedef IP, port ve kullanıcı adıyla bağlanın; WinPMEM ile RAM dump alınır.", "Connect with target IP, port and username; RAM dump is acquired with WinPMEM."),
    mode: "ssh-ram",
    output: "C:\\Amele\\Ciktilar"
  }
};
