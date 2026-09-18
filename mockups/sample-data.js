/** Shared sample diffs for dhcpdiff UI mockups (from test.log-style data). */
const SAMPLE_COUNTS = { all: 8, missing: 3, extra: 2, changed: 2, unmapped: 1 };

const SAMPLE_DIFFS = [
  {
    id: 0,
    category: "missing",
    categoryLabel: "MISSING",
    kind: "pool",
    entityKey: "pool:10.0.80.0/24:10.0.80.106-10.0.80.113",
    detail: "pool 10.0.80.106-10.0.80.113 missing in target — source splits this subnet into several ranges; target has one contiguous pool.",
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: `subnet 10.0.80.0 netmask 255.255.255.0 {
  pool {
    range 10.0.80.106 10.0.80.113;
    option routers 10.0.80.1;
    option domain-name-servers 10.0.1.10;
  }
  pool {
    range 10.0.80.115 10.0.80.140;
    option routers 10.0.80.1;
  }
}`,
    targetBlock: null,
    highlightLines: { source: [2, 3, 4, 5, 6] },
  },
  {
    id: 1,
    category: "extra",
    categoryLabel: "EXTRA",
    kind: "pool",
    entityKey: "pool:10.0.80.0/24:10.0.80.2-10.0.80.254",
    detail: "pool 10.0.80.2-10.0.80.254 extra in target — consolidated range covering addresses that appear as multiple pools in source.",
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: null,
    targetBlock: `subnet 10.0.80.0 netmask 255.255.255.0 {
  pool {
    range 10.0.80.2 10.0.80.254;
    option routers 10.0.80.1;
    option domain-name-servers 10.0.1.10;
    option domain-name "corp.example";
  }
}`,
    highlightLines: { target: [2, 3, 4, 5, 6, 7] },
  },
  {
    id: 2,
    category: "changed",
    categoryLabel: "CHANGED",
    kind: "option",
    entityKey: "option:pool:10.2.148.0/24:10.2.148.10-10.2.148.250:vci=PXEClient:dhcp:54 (dhcp-server-identifier)",
    detail: "dhcp:54 (dhcp-server-identifier): Ip(10.2.148.5) → Ip(10.2.18.95) under VCI scenario PXEClient.",
    sourceValue: "Ip(10.2.148.5)",
    targetValue: "Ip(10.2.18.95)",
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: `subnet 10.2.148.0 netmask 255.255.255.0 {
  pool {
    range 10.2.148.10 10.2.148.250;
    allow members of "PXEClient";
    option dhcp-server-identifier 10.2.148.5;
    option tftp-server-address 10.2.148.5;
    option bootfile-name "SMSBoot\\\\x64\\\\wdsmgfw.efi";
  }
}`,
    targetBlock: `subnet 10.2.148.0 netmask 255.255.255.0 {
  pool {
    range 10.2.148.10 10.2.148.250;
    allow members of "PXEClient";
    option dhcp-server-identifier 10.2.18.95;
    option tftp-server-address 10.2.18.95;
    option bootfile-name "SMSBoot\\\\x86\\\\wdsnbp.com";
  }
}`,
    highlightLines: { source: [5], target: [5] },
  },
  {
    id: 3,
    category: "changed",
    categoryLabel: "CHANGED",
    kind: "option",
    entityKey: "option:pool:10.2.148.0/24:10.2.148.10-10.2.148.250:vci=PXEClient:dhcp:67 (dhcp-bootfile-name)",
    detail: "dhcp:67 (dhcp-bootfile-name): String(\"SMSBoot\\\\x64\\\\wdsmgfw.efi\") → String(\"SMSBoot\\\\x86\\\\wdsnbp.com\").",
    sourceValue: 'String("SMSBoot\\\\x64\\\\wdsmgfw.efi")',
    targetValue: 'String("SMSBoot\\\\x86\\\\wdsnbp.com")',
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: `subnet 10.2.148.0 netmask 255.255.255.0 {
  pool {
    range 10.2.148.10 10.2.148.250;
    allow members of "PXEClient";
    option dhcp-server-identifier 10.2.148.5;
    option bootfile-name "SMSBoot\\\\x64\\\\wdsmgfw.efi";
  }
}`,
    targetBlock: `subnet 10.2.148.0 netmask 255.255.255.0 {
  pool {
    range 10.2.148.10 10.2.148.250;
    allow members of "PXEClient";
    option dhcp-server-identifier 10.2.18.95;
    option bootfile-name "SMSBoot\\\\x86\\\\wdsnbp.com";
  }
}`,
    highlightLines: { source: [6], target: [6] },
  },
  {
    id: 4,
    category: "missing",
    categoryLabel: "MISSING",
    kind: "subnet",
    entityKey: "subnet:192.168.111.0/24",
    detail: "subnet 192.168.111.0/24 missing in target.",
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: `subnet 192.168.111.0 netmask 255.255.255.0 {
  option routers 192.168.111.1;
  option domain-name-servers 10.0.1.10;
  pool {
    range 192.168.111.10 192.168.111.200;
  }
}`,
    targetBlock: null,
    highlightLines: { source: [1, 2, 3, 4, 5, 6, 7] },
  },
  {
    id: 5,
    category: "extra",
    categoryLabel: "EXTRA",
    kind: "subnet",
    entityKey: "subnet:192.168.140.0/24",
    detail: "subnet 192.168.140.0/24 extra in target.",
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: null,
    targetBlock: `subnet 192.168.140.0 netmask 255.255.255.0 {
  option routers 192.168.140.1;
  option domain-name "lab.example";
  pool {
    range 192.168.140.20 192.168.140.250;
  }
}`,
    highlightLines: { target: [1, 2, 3, 4, 5, 6, 7] },
  },
  {
    id: 6,
    category: "missing",
    categoryLabel: "MISSING",
    kind: "reservation",
    entityKey: "reservation:10.0.80.49",
    detail: "reservation 10.0.80.49 (00:17:c8:ca:73:f0) missing in target.",
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: `host printer-floor3 {
  hardware ethernet 00:17:c8:ca:73:f0;
  fixed-address 10.0.80.49;
  option host-name "printer-floor3";
}`,
    targetBlock: null,
    highlightLines: { source: [1, 2, 3, 4, 5] },
  },
  {
    id: 7,
    category: "unmapped",
    categoryLabel: "UNMAPPED",
    kind: "option",
    entityKey: "option:subnet:10.10.224.0/24:vendor:242 (Avaya-IP-Phone)",
    detail: "space:vendor code:242 — option name not mapped; run dhcpdiff map or add an alias in mappings/user.yaml.",
    sourceFile: "nssds_sc_15_16.bcn.conf",
    targetFile: "nssds_sc_15_16.ibx.conf",
    sourceBlock: `subnet 10.10.224.0 netmask 255.255.255.0 {
  option Avaya-IP-Phone "MCIPADD=10.10.1.5,MCPORT=1719";
  option routers 10.10.224.1;
}`,
    targetBlock: `subnet 10.10.224.0 netmask 255.255.255.0 {
  # option 242 present under a different vendor name
  option option-242 "MCIPADD=10.10.1.5,MCPORT=1719";
  option routers 10.10.224.1;
}`,
    highlightLines: { source: [2], target: [3] },
  },
];

const SAMPLE_SOURCE_FILE = `# BlueCat export (excerpt)
# nssds_sc_15_16.bcn.conf

subnet 10.0.80.0 netmask 255.255.255.0 {
  pool {
    range 10.0.80.106 10.0.80.113;
    option routers 10.0.80.1;
    option domain-name-servers 10.0.1.10;
  }
  pool {
    range 10.0.80.115 10.0.80.140;
    option routers 10.0.80.1;
  }
}

host printer-floor3 {
  hardware ethernet 00:17:c8:ca:73:f0;
  fixed-address 10.0.80.49;
  option host-name "printer-floor3";
}

subnet 10.2.148.0 netmask 255.255.255.0 {
  pool {
    range 10.2.148.10 10.2.148.250;
    allow members of "PXEClient";
    option dhcp-server-identifier 10.2.148.5;
    option tftp-server-address 10.2.148.5;
    option bootfile-name "SMSBoot\\\\x64\\\\wdsmgfw.efi";
  }
}

subnet 192.168.111.0 netmask 255.255.255.0 {
  option routers 192.168.111.1;
  option domain-name-servers 10.0.1.10;
  pool {
    range 192.168.111.10 192.168.111.200;
  }
}

subnet 10.10.224.0 netmask 255.255.255.0 {
  option Avaya-IP-Phone "MCIPADD=10.10.1.5,MCPORT=1719";
  option routers 10.10.224.1;
}
`;

const SAMPLE_TARGET_FILE = `# Infoblox export (excerpt)
# nssds_sc_15_16.ibx.conf

subnet 10.0.80.0 netmask 255.255.255.0 {
  pool {
    range 10.0.80.2 10.0.80.254;
    option routers 10.0.80.1;
    option domain-name-servers 10.0.1.10;
    option domain-name "corp.example";
  }
}

subnet 10.2.148.0 netmask 255.255.255.0 {
  pool {
    range 10.2.148.10 10.2.148.250;
    allow members of "PXEClient";
    option dhcp-server-identifier 10.2.18.95;
    option tftp-server-address 10.2.18.95;
    option bootfile-name "SMSBoot\\\\x86\\\\wdsnbp.com";
  }
}

subnet 192.168.140.0 netmask 255.255.255.0 {
  option routers 192.168.140.1;
  option domain-name "lab.example";
  pool {
    range 192.168.140.20 192.168.140.250;
  }
}

subnet 10.10.224.0 netmask 255.255.255.0 {
  # option 242 present under a different vendor name
  option option-242 "MCIPADD=10.10.1.5,MCPORT=1719";
  option routers 10.10.224.1;
}
`;

const SAMPLE_FILE_HIGHLIGHTS = {
  0: { source: [4, 9], target: null },
  1: { source: null, target: [4, 11] },
  2: { source: [22, 30], target: [13, 21] },
  3: { source: [22, 30], target: [13, 21] },
  4: { source: [32, 38], target: null },
  5: { source: null, target: [23, 29] },
  6: { source: [16, 20], target: null },
  7: { source: [40, 43], target: [31, 35] },
};
