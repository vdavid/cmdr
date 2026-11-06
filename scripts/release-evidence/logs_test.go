package main

import (
	"reflect"
	"testing"
)

func TestLoggedJobIDsSkipsJobsThatNeverRan(t *testing.T) {
	jobs := `{"jobs":[{"databaseId":1,"conclusion":"success"},{"databaseId":2,"conclusion":"skipped"},{"databaseId":3,"conclusion":"failure"}]}`
	got, err := loggedJobIDs([]byte(jobs))
	if err != nil {
		t.Fatal(err)
	}
	if want := []int64{1, 3}; !reflect.DeepEqual(got, want) {
		t.Fatalf("got %v, want %v", got, want)
	}
}

func TestLoggedJobIDsRefusesBrokenJSON(t *testing.T) {
	if _, err := loggedJobIDs([]byte("not json")); err == nil {
		t.Fatal("want an error for unreadable jobs JSON")
	}
}
