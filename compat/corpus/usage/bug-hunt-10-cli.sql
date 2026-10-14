-- case: bug-hunt-10/cli/zero-argument-strftime-and-geopoly-svg
SELECT typeof(strftime());
SELECT typeof(geopoly_svg());
SELECT strftime() IS NULL, geopoly_svg() IS NULL;
