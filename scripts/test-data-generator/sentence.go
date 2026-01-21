// Word lists and sentence generation for test data.
package main

import (
	"fmt"
	"math/rand"
	"time"
)

// Word lists for sentence generation - picked for maximum entertainment value

var names = []string{
	"David", "Gertrude", "Chad", "Beatrice", "Wolfgang", "Thomas", "Bartholomew", "Helga",
	"Donald", "Mildred", "Cornelius", "Julia", "Archibald", "Edith", "Montgomery", "Gladys",
	"Willy", "Brunhilde", "Percival", "Agatha",
}

var verbsPast = []string{
	"devoured", "grated", "befriended", "interrogated", "serenaded",
	"catapulted", "photobombed", "ghosted", "rickrolled", "bamboozled",
}

var verbsPresent = []string{
	"eats", "greets", "befriends", "interrogates", "serenades",
	"catapults", "photobombs", "ghosts", "rickrolls", "bamboozles",
}

var verbsFuture = []string{
	"will devour", "will say goodbye to", "will befriend", "will interrogate", "will serenade",
	"will catapult", "will photobomb", "will ghost", "will rickroll", "will bamboozle",
}

var articles = []string{"a", "the"}

// Adverbs starting with consonant (to match "a")
var adverbs = []string{
	"suspiciously", "dramatically", "rather", "quite", "passionately",
	"massively", "mysteriously", "aggressively", "surprisingly", "sarcastically",
}

var positiveAdjectives = []string{
	"magnificent", "glorious", "spectacular", "fabulous", "majestic",
	"legendary", "pristine", "exquisite", "splendid", "divine",
	"radiant", "dazzling", "illustrious", "sublime", "phenomenal",
	"resplendent", "sumptuous", "transcendent", "nice", "wondrous",
}

var conjunctions = []string{"but", "and"}

var negativeAdjectives = []string{
	"cursed", "suspicious", "questionable", "haunted", "soggy",
	"expired", "possessed", "radioactive", "sentient", "vengeful",
	"chaotic", "forbidden", "unhinged", "ominous", "volatile",
	"malevolent", "treacherous", "diabolical", "nefarious", "apocalyptic",
}

var objects = []string{
	"banana", "kazoo", "rubber duck", "burrito", "accordion",
	"sock puppet", "disco ball", "potato", "chainsaw", "unicycle",
	"trombone", "waffle iron", "lawn flamingo", "fog machine", "cheese wheel",
	"bagpipe", "lava lamp", "taco", "hedge trimmer", "bowling ball",
	"theremin", "cactus", "sousaphone", "meatball", "submarine",
	"anvil", "pickle jar", "trampoline", "baguette", "jetpack",
	"saxophone", "watermelon", "catapult", "chandelier", "harmonica",
	"wheelbarrow", "croissant", "pogo stick", "xylophone", "spatula",
	"didgeridoo", "pretzel", "hovercraft", "gargoyle", "ukulele",
	"jackhammer", "pancake", "trebuchet", "gnome statue", "kazoo army",
}

// generateSentence creates a random humorous sentence.
// Structure: "{Name} {verb} {article} {adverb} {positive adj} {and/but} {adverb} {negative adj} {object}."
// Example: "Gertrude is yeeting a suspiciously magnificent but dramatically cursed rubber duck."
func generateSentence() string {
	// Pick random tense
	var verb string
	switch rand.Intn(3) {
	case 0:
		verb = verbsPast[rand.Intn(len(verbsPast))]
	case 1:
		verb = verbsPresent[rand.Intn(len(verbsPresent))]
	default:
		verb = verbsFuture[rand.Intn(len(verbsFuture))]
	}

	return fmt.Sprintf("%s %s %s %s %s %s %s %s %s.",
		names[rand.Intn(len(names))],
		verb,
		articles[rand.Intn(len(articles))],
		adverbs[rand.Intn(len(adverbs))],
		positiveAdjectives[rand.Intn(len(positiveAdjectives))],
		conjunctions[rand.Intn(len(conjunctions))],
		adverbs[rand.Intn(len(adverbs))],
		negativeAdjectives[rand.Intn(len(negativeAdjectives))],
		objects[rand.Intn(len(objects))],
	)
}

// generateTimestamp returns a random timestamp between 2030-01-01 and 2040-01-01.
func generateTimestamp() time.Time {
	start := time.Date(2030, 1, 1, 0, 0, 0, 0, time.UTC)
	end := time.Date(2040, 1, 1, 0, 0, 0, 0, time.UTC)
	delta := end.Sub(start)
	randomDuration := time.Duration(rand.Int63n(int64(delta)))
	return start.Add(randomDuration)
}
