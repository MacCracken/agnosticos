#!/usr/bin/env bash
#
# mbp-arp-rx-probe.sh — on-LAN unicast-TCP RX probe, run from a macOS MBP.
#
# WHY THIS EXISTS (rewritten 2026-05-25, agnos 1.32.7 bite-2)
#   The whole 1.32.x arc has converged on ONE blocker: AGNOS-on-archaemenid
#   (r8169) receives broadcast + multicast frames (they ride the MAR all-1s
#   hash) but has never been proven to receive a UNICAST frame addressed to
#   its own MAC (the IDR physical-match path). Every external test bottoms out
#   the same way:  L2 OK (gateway MAC learned by broadcast snoop)  ->  tcp
#   connect 1.1.1.1:80  ->  "SYN sent but no SYN+ACK", because the SYN+ACK
#   comes back through the gateway as a unicast frame to AGNOS's MAC and the
#   NIC drops it. A Linux SYN probe already proved 1.1.1.1 *does* put that
#   SYN+ACK on the wire, so the fault is unicast-class RX on iron.
#
#   The 1.1.1.1 path has confounds we don't control: NAT, the gateway, the
#   upstream provider, Cloudflare. This probe removes ALL of them by making
#   THIS MBP the TCP server on the SAME L2 segment and having AGNOS connect to
#   it directly. We own both endpoints and log exactly what crosses the wire.
#
#   THE MECHANISM (zero unicast RX is needed until the decisive moment):
#     1. We ping AGNOS (192.168.1.222) -> macOS broadcasts "who-has .222,
#        tell <this MBP>". AGNOS's net_handle_arp snoops the *sender* (our IP
#        + MAC) from that BROADCAST and records us as its on-LAN peer (RFC 826
#        sender-snoop, the same path it learns the gateway). NO unicast RX.
#     2. AGNOS connects to <this MBP>:80 over the same segment -- its SYN
#        egresses (TX is proven 4x over).
#     3. THIS MBP's kernel answers with a SYN+ACK addressed UNICAST to AGNOS's
#        MAC. That is the FIRST and ONLY unicast frame AGNOS must receive.
#     4a. If AGNOS sends the completing ACK (our listener accepts the
#         connection / the pcap shows the 3rd-handshake ACK) -> r8169 unicast
#         RX WORKS. Then the 1.1.1.1 failure is gateway/off-LAN-specific, not
#         raw unicast RX -> big re-scope.
#     4b. If the pcap shows our SYN+ACK went out but AGNOS never ACKs ->
#         unicast-RX drop CONFIRMED at an endpoint we own and log, with no
#         gateway/NAT/Cloudflare doubt left.
#
#   The on-LAN SYN+ACK is the SAME IDR-physical-match RX class as the
#   gateway-relayed one, so given the current understanding we EXPECT 4b. The
#   value is an unimpeachable, controllable proof + a reusable harness: once
#   unicast RX is fixed, this is the regression test that turns green first.
#
#   AGNOS-side support (agnos 1.32.7 bite-2): net_handle_arp captures the
#   non-gateway who-has-for-us sender into net_peer_*; route_next_hop_mac
#   resolves on-LAN to it; main.cyr runs "tcp: connect on-LAN <peer>:80" and
#   prints  net: LAN-TCP OK / FAIL  BEFORE the existing 1.1.1.1 test. So the
#   framebuffer shows BOTH verdicts in one boot.
#
#   Channels per project rules: framebuffer boot log + this wire capture +
#   this MBP's listener log. No CMOS, no serial.
#
# TIMING
#   AGNOS polls the NIC during its ~5 s boot ARP window, then opens a ~2 s
#   peer-wait, then connects. The stimulus below blasts who-has .222
#   continuously and fires an extra burst the instant it sees AGNOS's own boot
#   ARP, so AGNOS captures us as its peer in-window.
#
# USAGE
#   chmod +x mbp-arp-rx-probe.sh
#   sudo ./mbp-arp-rx-probe.sh                 # default 120 s capture
#   sudo ./mbp-arp-rx-probe.sh 180             # custom capture seconds
#   ...then power on / reboot archaemenid into AGNOS while it runs. Ctrl-C to
#   stop early; the pcap + listener log + verdict are still produced.
#
#   WI-FI IS FINE. The MBP is Wi-Fi-only and that's OK: archaemenid is wired,
#   both are in 192.168.1.0/24, and the AP bridges wired<->wireless at L2 in
#   the same subnet, so AGNOS's SYN and our SYN+ACK are ordinary same-segment
#   unicast frames (no gateway routing). Prior captures already proved this
#   MBP both receives archaemenid's broadcasts and has bidirectional unicast
#   with it (SSH) -- exactly the two capabilities this probe exercises.
#
#   macOS firewall: if the built-in application firewall is in "block all" /
#   stealth mode it can drop inbound SYNs to the listener. If the pcap shows
#   AGNOS's SYN arriving but NO SYN+ACK leaving this MBP, allow inbound on the
#   listener port (System Settings -> Network -> Firewall) and re-run.

set -euo pipefail

# ---- target constants (from the burn under test) ---------------------------
AGNOS_MAC="b0:41:6f:0c:e4:25"     # archaemenid r8169 EEPROM MAC
AGNOS_IP="192.168.1.222"          # AGNOS static IP (net.cyr net_ip)
LISTEN_PORT=80                    # AGNOS connects to <this MBP>:80 (main.cyr)
DURATION="${1:-120}"             # capture seconds

# ---- must be root (tcpdump + arp -d + bind :80 need it) --------------------
if [[ "$(id -u)" -ne 0 ]]; then
  echo "Re-run with sudo:  sudo $0 ${1:-}" >&2
  exit 1
fi

# ---- pick the interface that routes to AGNOS's subnet ----------------------
IFACE="$(route -n get "$AGNOS_IP" 2>/dev/null | awk '/interface:/{print $2}')"
if [[ -z "${IFACE:-}" ]]; then
  echo "Could not determine egress interface for $AGNOS_IP." >&2
  echo "List interfaces with:  ifconfig -a   then set IFACE manually." >&2
  exit 1
fi
MYIP="$(ipconfig getifaddr "$IFACE" 2>/dev/null || true)"
MYMAC="$(ifconfig "$IFACE" 2>/dev/null | awk '/^[[:space:]]*ether /{print $2; exit}')"
if [[ -z "${MYIP:-}" ]]; then
  echo "Could not read this MBP's IPv4 on $IFACE — is Wi-Fi up on 192.168.1.0/24?" >&2
  exit 1
fi

# ---- python3 is required for the logging TCP listener ----------------------
if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 not found — install Command Line Tools: xcode-select --install" >&2
  exit 1
fi

TS="$(date +%Y%m%d-%H%M%S)"
PCAP="agnos-lan-probe-${TS}.pcapng"
LIVELOG="agnos-lan-probe-${TS}.log"
SRVLOG="agnos-lan-server-${TS}.log"

echo "=============================================================="
echo " AGNOS r8169 on-LAN unicast-TCP RX probe (macOS)"
echo "   interface : $IFACE   (this MBP: $MYIP @ ${MYMAC:-unknown})"
echo "   server    : 0.0.0.0:$LISTEN_PORT   <-- AGNOS should connect here"
echo "   target    : $AGNOS_IP @ $AGNOS_MAC"
echo "   capture   : $DURATION s  ->  $PCAP"
echo "--------------------------------------------------------------"
echo "   AGNOS auto-discovers THIS MBP ($MYIP) as its on-LAN peer from"
echo "   the who-has $AGNOS_IP broadcasts the stimulus below emits."
echo "   Watch AGNOS's framebuffer for:"
echo "     tcp: connect on-LAN $MYIP:80"
echo "     net: LAN-TCP OK  -> r8169 unicast RX WORKS"
echo "     net: LAN-TCP FAIL -> unicast RX drop (we logged the SYN+ACK)"
echo "=============================================================="
echo
echo ">>> POWER ON / REBOOT archaemenid into AGNOS NOW. <<<"
echo "    (Ctrl-C to stop early; pcap + logs + verdict are still produced.)"
echo

# ---- the "webservice": a logging TCP listener on :80 -----------------------
# A bound listener makes the macOS kernel auto-answer AGNOS's SYN with a
# SYN+ACK (that auto-response is what tests AGNOS's unicast RX). On a COMPLETED
# handshake we also accept(), log the peer, and return a tiny HTTP 200 — so a
# line in $SRVLOG is the definitive "unicast RX works" signal (the pcap is the
# forensic backup). Listens IPv4 on all addresses so AGNOS's .222->MYIP SYN
# lands regardless of which local address it targets.
listener() {
  LISTEN_PORT="$LISTEN_PORT" SRVLOG="$SRVLOG" python3 - <<'PY'
import os, socket, datetime, sys
port = int(os.environ["LISTEN_PORT"])
logf = open(os.environ["SRVLOG"], "a", buffering=1)
def log(m):
    logf.write("%s %s\n" % (datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3], m))
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    s.bind(("0.0.0.0", port))
except OSError as e:
    log("BIND FAILED on :%d (%s) -- is something already on the port?" % (port, e))
    sys.exit(1)
s.listen(8)
log("listener up on 0.0.0.0:%d" % port)
while True:
    try:
        conn, addr = s.accept()
    except Exception as e:
        log("accept error: %s" % e); continue
    log("ACCEPTED connection from %s:%d  <-- COMPLETED handshake: unicast RX OK" % addr)
    try:
        conn.settimeout(2.0)
        data = conn.recv(1024)
        if data:
            first = data.split(b"\r\n", 1)[0].decode("latin-1", "replace")
            log("  request from %s:%d : %r" % (addr[0], addr[1], first))
        body = b"AGNOS on-LAN unicast RX OK\n"
        conn.sendall(b"HTTP/1.0 200 OK\r\nContent-Length: %d\r\nConnection: close\r\n\r\n%s"
                     % (len(body), body))
    except Exception as e:
        log("  post-accept I/O note from %s:%d : %s" % (addr[0], addr[1], e))
    finally:
        conn.close()
PY
}

# ---- background stimulus: keep broadcasting who-has 192.168.1.222 ----------
# This is now LOAD-BEARING (not just elicitation): the who-has carries our IP
# + MAC as sender, which AGNOS snoops to learn us as its on-LAN peer. arp -d
# before each ping forces a fresh broadcast (defeats the macOS ARP throttle).
stimulus() {
  while :; do
    arp -d "$AGNOS_IP" >/dev/null 2>&1 || true
    ping -c 1 -t 1 "$AGNOS_IP" >/dev/null 2>&1 || true
    sleep 0.3
  done
}

cleanup() {
  [[ -n "${STIM_PID:-}" ]]  && kill "$STIM_PID"  2>/dev/null || true
  [[ -n "${SRV_PID:-}" ]]   && kill "$SRV_PID"   2>/dev/null || true
  [[ -n "${TCPDW_PID:-}" ]] && kill "$TCPDW_PID" 2>/dev/null || true
  [[ -n "${TCPD_PID:-}" ]]  && kill "$TCPD_PID"  2>/dev/null || true
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# raw pcap to disk (full frames) for the durable record / later audit
tcpdump -i "$IFACE" -w "$PCAP" -U "arp or ether host $AGNOS_MAC" >/dev/null 2>&1 &
TCPDW_PID=$!

listener & SRV_PID=$!
stimulus & STIM_PID=$!

# live decoded stream we both show and react to
tcpdump -i "$IFACE" -nn -e -l "arp or ether host $AGNOS_MAC" 2>/dev/null | tee "$LIVELOG" | \
while IFS= read -r line; do
  echo "$line"
  # AGNOS just emitted its boot ARP -> its RX poll window is OPEN. Fire a burst
  # of who-has .222 so AGNOS snoops us as its peer well inside the window.
  if [[ "$line" == *"$AGNOS_MAC > ff:ff:ff:ff:ff:ff"* ]]; then
    echo "    [trigger] AGNOS boot ARP seen -> firing who-has $AGNOS_IP burst (peer-discovery)"
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      arp -d "$AGNOS_IP" >/dev/null 2>&1 || true
      ping -c 1 -t 1 "$AGNOS_IP" >/dev/null 2>&1 || true
    done
  fi
  # AGNOS answered our who-has -> broadcast RX + ARP responder confirmed (again).
  if [[ "$line" == *"$AGNOS_MAC"* && "$line" == *"is-at $AGNOS_MAC"* ]]; then
    echo "    [ok] AGNOS replied to our who-has -> broadcast RX + ARP responder live."
  fi
  # AGNOS sent us a TCP SYN -> peer discovery worked AND TX of the SYN worked.
  if [[ "$line" == *"$AGNOS_IP."* && "$line" == *"> $MYIP.$LISTEN_PORT:"* && "$line" == *"Flags [S]"* ]]; then
    echo "    [!!] AGNOS sent a TCP SYN to $MYIP:$LISTEN_PORT -> it discovered us + TX'd the SYN."
    echo "         Our kernel will SYN+ACK; watch for an ACCEPTED line / AGNOS's ACK below."
  fi
  # The completing ACK or data from AGNOS -> it RECEIVED our unicast SYN+ACK.
  if [[ "$line" == *"$AGNOS_IP."* && "$line" == *"> $MYIP.$LISTEN_PORT:"* ]] && \
     { [[ "$line" == *"Flags [.]"* ]] || [[ "$line" == *"Flags [P."* ]]; }; then
    echo "    [!!] AGNOS ACK'd/sent data to $MYIP:$LISTEN_PORT -> it RECEIVED our unicast"
    echo "         SYN+ACK. r8169 UNICAST RX WORKS."
  fi
done &
TCPD_PID=$!

# ---- run for the window ----------------------------------------------------
sleep "$DURATION" &
SLEEP_PID=$!
wait "$SLEEP_PID" 2>/dev/null || true   # interruptible by Ctrl-C (trap cleans up)

cleanup
trap - EXIT INT TERM
sleep 0.5   # let tcpdump flush the pcap

# ---- verdict from the pcap + listener log ----------------------------------
MYREQ=$(tcpdump -nn -e -r "$PCAP" "arp" 2>/dev/null \
      | grep -c "who-has $AGNOS_IP" || true)
AGNOS_BOOT_ARP=$(tcpdump -nn -e -r "$PCAP" "arp" 2>/dev/null \
      | grep -F "$AGNOS_MAC > ff:ff:ff:ff:ff:ff" | grep -c "who-has" || true)
AGNOS_ARP_REPLY=$(tcpdump -nn -e -r "$PCAP" "arp" 2>/dev/null \
      | grep -c "is-at $AGNOS_MAC" || true)
SYN_IN=$(tcpdump -nn -r "$PCAP" "tcp port $LISTEN_PORT" 2>/dev/null \
      | grep -F "$AGNOS_IP." | grep -F "> $MYIP.$LISTEN_PORT:" | grep -c "Flags \[S\]" || true)
SYNACK_OUT=$(tcpdump -nn -r "$PCAP" "tcp port $LISTEN_PORT" 2>/dev/null \
      | grep -F "$MYIP.$LISTEN_PORT > $AGNOS_IP." | grep -c "Flags \[S\.\]" || true)
ACK_IN=$(tcpdump -nn -r "$PCAP" "tcp port $LISTEN_PORT" 2>/dev/null \
      | grep -F "$AGNOS_IP." | grep -F "> $MYIP.$LISTEN_PORT:" \
      | grep -cE "Flags \[(\.|P\.)\]" || true)
ACCEPTED=0
[[ -f "$SRVLOG" ]] && ACCEPTED=$(grep -c "ACCEPTED connection" "$SRVLOG" || true)

echo
echo "=============================================================="
echo " VERDICT  (pcap: $PCAP   server log: $SRVLOG)"
echo "--------------------------------------------------------------"
echo "  our who-has $AGNOS_IP sent (peer-discovery stimulus) : $MYREQ"
echo "  AGNOS boot ARP egress (TX confirmed)               : $AGNOS_BOOT_ARP"
echo "  AGNOS ARP reply to our who-has (broadcast RX)      : $AGNOS_ARP_REPLY"
echo "  AGNOS -> us  TCP SYN  (peer discovered + SYN TX)   : $SYN_IN"
echo "  us -> AGNOS  TCP SYN+ACK (our kernel answered)     : $SYNACK_OUT"
echo "  AGNOS -> us  ACK / data (UNICAST RX of SYN+ACK)    : $ACK_IN"
echo "  listener ACCEPTED connections (handshake complete) : $ACCEPTED"
echo "--------------------------------------------------------------"
if   ((ACCEPTED>0 || ACK_IN>0)); then
  echo "  -> AGNOS COMPLETED the on-LAN TCP handshake."
  echo "     r8169 UNICAST RX WORKS. The 1.1.1.1 failure is therefore"
  echo "     gateway/off-LAN-specific, NOT raw unicast RX -> RE-SCOPE."
  echo "     Confirm on the framebuffer:  net: LAN-TCP OK"
elif ((SYN_IN>0 && SYNACK_OUT>0)); then
  echo "  -> AGNOS sent the SYN, our kernel sent the SYN+ACK to AGNOS's MAC,"
  echo "     and AGNOS never ACK'd. UNICAST-RX DROP CONFIRMED at an endpoint"
  echo "     we own and logged (no gateway/NAT/Cloudflare in the path)."
  echo "     Framebuffer should read:  net: LAN-TCP FAIL"
elif ((SYN_IN>0)); then
  echo "  -> AGNOS sent the SYN but we logged NO SYN+ACK leaving this MBP."
  echo "     Likely the macOS firewall dropped the inbound SYN. Allow inbound"
  echo "     on :$LISTEN_PORT (Settings -> Network -> Firewall) and re-run."
elif ((AGNOS_ARP_REPLY>0)); then
  echo "  -> AGNOS answered our who-has (broadcast RX live) but sent no SYN."
  echo "     Peer discovery or on-LAN routing didn't fire. Check the FB for"
  echo "     'tcp: connect on-LAN $MYIP:80'; confirm this is agnos 1.32.7 bite-2+."
elif ((AGNOS_BOOT_ARP>0)); then
  echo "  -> AGNOS booted in-window (boot ARP seen) but never answered our"
  echo "     who-has. Broadcast RX didn't deliver this run -- unexpected at"
  echo "     1.32.6+; re-run, and confirm the build."
else
  echo "  -> No AGNOS traffic captured. It likely booted outside the window or"
  echo "     this is the wrong interface. Re-run; START THE CAPTURE BEFORE boot."
fi
echo "=============================================================="
echo "Artifacts: $PCAP (frames),  $LIVELOG (decoded stream),  $SRVLOG (server)"
