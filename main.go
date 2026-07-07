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
	// 1. Avataan Windowsin TAP-kortti
	config := water.Config{DeviceType: water.TAP}
	config.Name = "FC-TAP"
	tapDevice, err := water.New(config)
	if err != nil {
		log.Fatal("TAP-kortin avaaminen epäonnistui: ", err)
	}
	defer tapDevice.Close()

	ctx := context.Background()

	// 2. Alustetaan Libp2p-host automaattisella NAT-läpäisyllä
	h, err := libp2p.New(
		libp2p.NATPortMap(),      // Yrittää avata UPnP-portit automaattisesti
		libp2p.EnableNATService(),// Aktivoi NAT-läpäisyominaisuudet
		libp2p.EnableRelay(),     // Jos suora Hole Punching epäonnistuu, käyttää varapalvelinta (kuten DERP/TURN)
	)
	if err != nil {
		log.Fatal(err)
	}
	defer h.Close()

	// Tulostetaan osoite, jonka voit kopioida kaverille
	printAddresses(h)

	// 3. Määritetään mitä tehdään, kun kaveri ottaa yhteyden meihin
	h.SetStreamHandler(protocolID, func(stream network.Stream) {
		fmt.Println("Kaveri yhdisti! Tunneli valmis.")
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
		fmt.Println("Yhteys muodostettu! Tunneli valmis.")
		startBridging(tapDevice, stream)
	}

	// Pidetään ohjelma käynnissä
	select {}
}

func startBridging(tap *water.Interface, stream network.Stream) {
	// Lanka A: TAP -> P2P Netti
	go func() {
		buf := make([]byte, 2000)
		for {
			n, err := tap.Read(buf)
			if err == nil {
				stream.Write(buf[:n])
			}
		}
	}()

	// Lanka B: P2P Netti -> TAP
	bufIn := make([]byte, 2000)
	for {
		n, err := stream.Read(bufIn)
		if err == nil {
			tap.Write(bufIn[:n])
		} else if err == io.EOF {
			fmt.Println("Yhteys katkesi.")
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
