package config

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"sync"

	invjsonschema "github.com/invopop/jsonschema"
	jschema "github.com/santhosh-tekuri/jsonschema/v5"
	"sigs.k8s.io/yaml"

	schemadef "github.com/notwillk/workspace-doctor/schema"
)

const schemaResource = "https://workspace-doctor/schema/config.json"

var (
	compiledSchema *jschema.Schema
	schemaOnce     sync.Once
	schemaErr      error
)

// ResolvePath determines which config file should be used.
// If an explicit path is provided it must exist. Otherwise the well-known
// filenames `.workspace-doctor.yaml` and `.workspace-doctor.yml` are checked in
// the current working directory.
func ResolvePath(explicit string) (string, error) {
	if explicit != "" {
		if _, err := os.Stat(explicit); err != nil {
			return "", fmt.Errorf("config file %q: %w", explicit, err)
		}
		return explicit, nil
	}

	candidates := []string{".workspace-doctor.yaml", ".workspace-doctor.yml"}
	for _, candidate := range candidates {
		info, err := os.Stat(candidate)
		if err == nil {
			if info.IsDir() {
				return "", fmt.Errorf("config file %q: is a directory", candidate)
			}
			return candidate, nil
		}
		if !errors.Is(err, os.ErrNotExist) {
			return "", fmt.Errorf("config file %q: %w", candidate, err)
		}
	}

	return "", nil
}

// Load reads, validates, and unmarshals the workspace configuration.
func Load(path string) (*schemadef.Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read config: %w", err)
	}

	jsonData, err := yaml.YAMLToJSON(data)
	if err != nil {
		return nil, fmt.Errorf("convert config to JSON: %w", err)
	}

	var jsonValue interface{}
	if err := json.Unmarshal(jsonData, &jsonValue); err != nil {
		return nil, fmt.Errorf("decode config JSON: %w", err)
	}

	schema, err := compiledJSONSchema()
	if err != nil {
		return nil, err
	}

	if err := schema.Validate(jsonValue); err != nil {
		return nil, fmt.Errorf("config validation failed: %w", err)
	}

	var cfg schemadef.Config
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("decode config: %w", err)
	}

	return &cfg, nil
}

func compiledJSONSchema() (*jschema.Schema, error) {
	schemaOnce.Do(func() {
		reflected := invjsonschema.Reflect(&schemadef.Config{})
		payload, err := json.Marshal(reflected)
		if err != nil {
			schemaErr = fmt.Errorf("marshal schema: %w", err)
			return
		}

		compiler := jschema.NewCompiler()
		if err := compiler.AddResource(schemaResource, bytes.NewReader(payload)); err != nil {
			schemaErr = fmt.Errorf("load schema resource: %w", err)
			return
		}

		compiledSchema, schemaErr = compiler.Compile(schemaResource)
	})

	return compiledSchema, schemaErr
}
