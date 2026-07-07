package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"os"

	"github.com/libp2p/go-libp2p"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/network"
	"github.com/libp2p/go-libp2p/core/peer"
	"github.com/multiformats/go-multiaddr"
	"github.com/songgao/water"
)

const protocolID = "/fcvpn/1.0.0"

func main() {
	// 1. Avataan Windowsin TAP-kortti oikeilla Windows-asetuksilla
	config := water.Config{DeviceType: water.TAP}
	config.PlatformSpecificParams = water.PlatformSpecificParams{
		ComponentID:   "tap0901", // OpenVPN:n TAP-ajurin standarditunnus
		InterfaceName: "FC-TAP",
	}

	tapDevice, err := water.New(config)
	if err != nil {
		log.Fatal("TAP-kortin avaaminen epäonnistui (muistitko ajaa Adminina ja onko FC-TAP luotu?): ", err)
	}
	defer tapDevice.Close()

	ctx := context.Background()

	// 2. Alustetaan Libp2p-host automaattisella NAT-läpäisyllä
	h, err := libp2p.New(
		libp2p.NATPortMap(),       // Yrittää avata UPnP-portit automaattisesti
		libp2p.EnableNATService(), // Aktivoi NAT-läpäisyominaisuudet
		libp2p.EnableRelay(),      // Vararele, jos suora hole punching epäonnistuu
	)
	if err != nil {
		log.Fatal("Libp2p alustus epäonnistui: ", err)
	}
	defer h.Close()

	// Tulostetaan osoite, jonka voit kopioida kaverille
	printAddresses(h)

	// 3. Määritetään mitä tehdään, kun kaveri ottaa yhteyden meihin
	h.SetStreamHandler(protocolID, func(stream network.Stream) {
		fmt.Println("\n[TUNNELI] Kaveri yhdisti! Far Cry co-op pitäisi nyt toimia.")
		startBridging(tapDevice, stream)
	})

	// 4. Jos annoit kaverin osoitteen argumenttina, yhdistetään siihen
	if len(os.Args) > 1 {
		kaverinOsoite := os.Args[1]
		maddr, err := multiaddr.NewMultiaddr(kaverinOsoite)
		if err != nil {
			log.Fatal("Virheellinen osoite: ", err)
		}

		peerinfo, err := peer.AddrInfoFromP2pAddr(maddr)
		if err != nil {
			log.Fatal(err)
		}

		fmt.Println("Yhdistetään kaveriin...")
		if err := h.Connect(ctx, *peerinfo); err != nil {
			log.Fatal("Yhteys epäonnistui: ", err)
		}

		stream, err := h.NewStream(ctx, peerinfo.ID, protocolID)
		if err != nil {
			log.Fatal(err)
		}
		fmt.Println("\n[TUNNELI] Yhteys muodostettu kaveriin! Tunneli valmis.")
		startBridging(tapDevice, stream)
	}

	// Pidetään ohjelma käynnissä
	select {}
}

func startBridging(tap *water.Interface, stream network.Stream) {
	// Lanka A: Luetaan Far Cryn paketit TAP-kortilta ja ammutaan P2P-tunneliin
	go func() {
		buf := make([]byte, 2000)
		for {
			n, err := tap.Read(buf)
			if err == nil {
				_, _ = stream.Write(buf[:n])
			}
		}
	}()

	// Lanka B: Otetaan vastaan kaverin pelipaketit tunnelista ja syötetään Windowsille
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

func printAddresses(h host.Host) {
	fmt.Println("--------------------------------------------------")
	fmt.Println("Kopioi jokin näistä osoitteista kaverillesi:")
	for _, addr := range h.Addrs() {
		fmt.Printf("%s/p2p/%s\n", addr, h.ID())
	}
	fmt.Println("--------------------------------------------------")
}