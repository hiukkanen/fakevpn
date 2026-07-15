package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/libp2p/go-libp2p"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/network"
	"github.com/libp2p/go-libp2p/core/peer"
	"github.com/multiformats/go-multiaddr"
	"github.com/songgao/water"
)

const protocolID = "/fcvpn/1.0.0"

func main() {
	// 1. Avataan Windowsin TAP-kortti
	config := water.Config{DeviceType: water.TAP}
	config.PlatformSpecificParams = water.PlatformSpecificParams{
		ComponentID:   "tap0901", // OpenVPN TAP-ajuri
		InterfaceName: "FC-TAP",
	}

	tapDevice, err := water.New(config)
	if err != nil {
		log.Fatal("TAP-kortin avaaminen epäonnistui (muistitko ajaa Adminina ja onko FC-TAP luotu?): ", err)
	}
	defer tapDevice.Close()

	ctx := context.Background()

	// 2. Käynnistetään Libp2p-päätepiste automaattisella NAT-läpäisyllä ja relepalvelimilla
	h, err := libp2p.New(
		libp2p.NATPortMap(),      // Yrittää UPnP-porttiosoitusta
		libp2p.EnableAutoRelay(), // Ottaa käyttöön automaattisen välityspalvelimen (Relay) haun jos NAT on tiukka
		libp2p.EnableRelayService(),
	)
	if err != nil {
		log.Fatal("Verkkoalustus epäonnistui: ", err)
	}
	defer h.Close()

	// Tulostetaan osoitteet ja haetaan julkinen IP taustalla
	go printAddresses(h)

	// 3. Kuunnellaan tulevia yhteyksiä kaverilta
	h.SetStreamHandler(protocolID, func(stream network.Stream) {
		fmt.Println("\n[TUNNELI] Kaveri yhdisti! Tunneli valmis pelattavaksi.")
		startBridging(tapDevice, stream)
	})

	// 4. Jos komentorivillä annettiin kaverin osoite, yhdistetään siihen
	if len(os.Args) > 1 {
		kaverinOsoite := os.Args[1]
		maddr, err := multiaddr.NewMultiaddr(kaverinOsoite)
		if err != nil {
			log.Fatal("Virheellinen osoiteformaatti: ", err)
		}

		peerinfo, err := peer.AddrInfoFromP2pAddr(maddr)
		if err != nil {
			log.Fatal(err)
		}

		fmt.Println("Yhdistetään kaveriin (tämä voi kestää hetken)...")
		
		// Yritetään yhdistää useamman kerran taustalla
		var connErr error
		for i := 0; i < 3; i++ {
			if connErr = h.Connect(ctx, *peerinfo); connErr == nil {
				break
			}
			time.Sleep(2 * time.Second)
		}
		if connErr != nil {
			log.Fatal("Yhteys epäonnistui: ", connErr)
		}

		stream, err := h.NewStream(ctx, peerinfo.ID, protocolID)
		if err != nil {
			log.Fatal("Kanavan avaaminen epäonnistui: ", err)
		}
		
		fmt.Println("\n[TUNNELI] Yhteys muodostettu kaveriin! Tunneli valmis.")
		startBridging(tapDevice, stream)
	}

	// Pidetään ohjelma käynnissä
	select {}
}

// startBridging siirtää verkkoliikenteen livenä TAP-kortin ja tunnelin välillä
func startBridging(tap *water.Interface, stream network.Stream) {
	go func() {
		buf := make([]byte, 2000)
		for {
			n, err := tap.Read(buf)
			if err == nil {
				_, _ = stream.Write(buf[:n])
			}
		}
	}()

	bufIn := make([]byte, 2000)
	for {
		n, err := stream.Read(bufIn)
		if err == nil {
			_, _ = tap.Write(bufIn[:n])
		} else if err == io.EOF {
			fmt.Println("\n[TUNNELI] Yhteys katkesi.")
			return
		}
	}
}

func getPublicIP() string {
	client := http.Client{Timeout: 5 * time.Second}
	resp, err := client.Get("https://ident.me")
	if err != nil {
		return ""
	}
	defer resp.Body.Close()
	ipBytes, err := io.ReadAll(resp.Body)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(ipBytes))
}

func printAddresses(h host.Host) {
	// Odotetaan hetki, että reititin ja julkiset yhteydet alustetaan
	time.Sleep(2 * time.Second)

	fmt.Println("--------------------------------------------------")
	fmt.Println("Kopioi jokin näistä osoitteista kaverillesi:")

	// 1. Tulostetaan paikalliset osoitteet (ilman APIPA 169.x osoitteita)
	for _, addr := range h.Addrs() {
		if !strings.Contains(addr.String(), "169.254") && !strings.Contains(addr.String(), "127.0.0.1") {
			fmt.Printf("%s/p2p/%s\n", addr, h.ID())
		}
	}

	// 2. Tulostetaan valmis julkinen osoite
	publicIP := getPublicIP()
	if publicIP != "" {
		fmt.Printf("\n>>> SUOSITELTU JULKINEN OSOITE (Lähetä tämä kaverille!):\n")
		fmt.Printf("/ip4/%s/udp/4001/quic-v1/p2p/%s\n", publicIP, h.ID())
		fmt.Printf("/ip4/%s/tcp/4001/p2p/%s\n", publicIP, h.ID())
	}
	fmt.Println("--------------------------------------------------")
}
