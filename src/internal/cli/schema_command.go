package cli

import (
	"encoding/json"
	"flag"
	"fmt"

	invjsonschema "github.com/invopop/jsonschema"
	configschema "github.com/notwillk/workspace-doctor/schema"
)

func (r *RootCommand) runSchema(args []string) int {
	flags := flag.NewFlagSet("schema", flag.ContinueOnError)
	flags.SetOutput(r.stderr)

	var pretty bool
	flags.BoolVar(&pretty, "pretty", false, "pretty-print the JSON schema")

	if err := flags.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return 0
		}
		return 2
	}

	schema := invjsonschema.Reflect(&configschema.Config{})

	var (
		data []byte
		err  error
	)
	if pretty {
		data, err = json.MarshalIndent(schema, "", "  ")
	} else {
		data, err = json.Marshal(schema)
	}
	if err != nil {
		fmt.Fprintf(r.stderr, "failed to render schema: %v\n", err)
		return 2
	}

	data = append(data, '\n')
	if _, err := r.stdout.Write(data); err != nil {
		fmt.Fprintf(r.stderr, "failed to write schema: %v\n", err)
		return 2
	}

	return 0
}
