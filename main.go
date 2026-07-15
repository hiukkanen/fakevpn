package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"os"

	"git.coopcloud.tech/decentral1se/iroh-go"
	"github.com/songgao/water"
)

const protocolID = "fcvpn/1"

func main() {
	// 1. Avataan Windowsin TAP-kortti
	config := water.Config{DeviceType: water.TAP}
	config.PlatformSpecificParams = water.PlatformSpecificParams{
		ComponentID:   "tap0901",
		InterfaceName: "FC-TAP",
	}

	tapDevice, err := water.New(config)
	if err != nil {
		log.Fatal("TAP-kortin avaaminen epäonnistui (muistitko ajaa Adminina ja onko FC-TAP luotu?): ", err)
	}
	defer tapDevice.Close()

	ctx := context.Background()

	// 2. Alustetaan virallinen Iroh-solmu (Node) muistissa pyörivällä tietokannalla
	node, err := iroh.NewNode(ctx, iroh.DefaultNodeConfig())
	if err != nil {
		log.Fatal("Iroh-solmun alustus epäonnistui: ", err)
	}
	defer node.Shutdown(ctx)

	// Haetaan oma Node ID (Irohin yksilöllinen tunniste)
	nodeID, err := node.ID(ctx)
	if err != nil {
		log.Fatal("Node ID:n haku epäonnistui: ", err)
	}

	fmt.Println("--------------------------------------------------")
	fmt.Println("Kopioi tämä Iroh Node ID kaverillesi:")
	fmt.Printf("%s\n", nodeID.String())
	fmt.Println("--------------------------------------------------")

	// 3. Otetaan vastaan tulevat ALPN-yhteydet
	go func() {
		for {
			// Hyväksytään uusi yhteys Iroh-verkosta
			conn, err := node.Accept(ctx)
			if err != nil {
				continue
			}

			// Tarkistetaan täsmääkö protokolla
			if conn.ALPN() == protocolID {
				stream, err := conn.AcceptStream(ctx)
				if err == nil {
					fmt.Println("\n[TUNNELI] Kaveri yhdisti! Far Cry pitäisi nyt toimia.")
					startBridging(tapDevice, stream)
				}
			}
		}
	}()

	// 4. Jos annoit kaverin Node ID:n argumenttina, yhdistetään siihen
	if len(os.Args) > 1 {
		kaverinNodeIDStr := os.Args[1]
		kaverinNodeID, err := iroh.NodeIDFromString(kaverinNodeIDStr)
		if err != nil {
			log.Fatal("Virheellinen kaverin Node ID: ", err)
		}

		fmt.Println("Yhdistetään kaveriin Iroh-verkon kautta...")

		// Yhdistetään suoraan kaverin Node ID:hen. 
		// Iroh osaa etsiä oikean reitin ja käyttää DERP-relepalvelimia tarvittaessa!
		conn, err := node.Connect(ctx, kaverinNodeID, protocolID)
		if err != nil {
			log.Fatal("Yhteys epäonnistui: ", err)
		}

		stream, err := conn.OpenStream(ctx)
		if err != nil {
			log.Fatal("Kanavan avaaminen epäonnistui: ", err)
		}

		fmt.Println("\n[TUNNELI] Yhteys muodostettu kaveriin! Tunneli valmis.")
		startBridging(tapDevice, stream)
	}

	// Pidetään ohjelma käynnissä
	select {}
}

// startBridging siirtää paketit TAP-laitteen ja Iroh-streamin välillä
func startBridging(tap *water.Interface, stream iroh.Stream) {
	// Lanka A: Luetaan pelipaketit TAP-kortilta ja lähetetään kaverille
	go func() {
		buf := make([]byte, 2000)
		for {
			n, err := tap.Read(buf)
			if err == nil {
				_, _ = stream.Write(buf[:n])
			}
		}
	}()

	// Lanka B: Otetaan vastaan kaverin pelipaketit ja syötetään ne Windowsille
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
