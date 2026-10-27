-- Champions taken from upstream Reckless: the upstream commits each brought (JSON `UpstreamMerge`), null otherwise.
ALTER TABLE versions ADD COLUMN upstream TEXT;
