#!/usr/bin/env bash
# FilePath: scripts/guard/no-private-names.sh
# Keeps names from a real team's private Dictionary (company, product and people names, and the
# company's mail domains) out of the public repository. Tests, docs and eval fixtures use
# invented names; the real Dictionary is imported on each Mac and never committed.
#
# The terms are listed below only as SHA-256 hashes of their lowercase spelling, so this file
# does not publish them either. Every tracked text file is split into runs of ASCII letters,
# lowercased; a run matches when it is a listed whole word ("whole" hashes), when any part of it
# is a listed word ("part" hashes, which also catches glued forms such as a domain or a
# run-together name), or when it and the next run joined together are a listed two-word term
# ("pair" hashes). The maintainer's own `authors` line in Cargo.toml is the one exemption.
#
# To list another term: printf %s 'term' | shasum -a 256 (two words joined without the space
# for "pair"), and add the hash to "part", or to "whole" when the term is also a part of
# ordinary words.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

part_hashes="
0b5d7ed604761c95867a164e58b8911e3a918a364b9df84292b7271ee231e60d
35a1c19de3c6fa85f3b9b465ebdee62a293639380edfb9138d36478087ea8b5e
92622b1a643b76930eaefccea52a9d76adf0be6a6b4b540ede874df7eaeeb81d
c4079e4ff051fbe0c532fccce3e1e63b5a701bee595783843d4b10603d944af2
4b81d9c157b819532e78eab4ee305e3947602d8d444874cb4a9199731ec61adc
233711c88978b1c42d0fde5dda01e1422ee2f947f91c8df1ffa2fa3b8ad4cd9f
ab1f7b7600761ef53800ccb5fd8b18827e4e7f191534d11a33ebbfa5482b767f
68dcb7a229029603231022e599ca688e818706aa4c375a7c22d83f89da094b3e
6fe119a2c938b33b1289c4bdfe3ff9e82ee22fa6efac2600cc4fe01027dabd6d
da22d5448d44e36f7e4f32976c4a43a8bb8e4646fc387baf2d989438198c6f49
0b3659243e3eb91cd2da518ebbad79997209e1284fe70756b3e8b8bc8701ac83
"
whole_hashes="
73eec471ee9b7cdee71c0d95776c8650d1c535ab398acd15742a1971164a30d0
837f8527e1a520cf88f414980e8e0dddb02e3672d8fca2aa3f79eca8fc20a764
"
pair_hashes="
92622b1a643b76930eaefccea52a9d76adf0be6a6b4b540ede874df7eaeeb81d
"

count=$(git ls-files -z | PART="$part_hashes" WHOLE="$whole_hashes" PAIR="$pair_hashes" perl -0 -ne '
    use strict;
    use warnings;
    use Digest::SHA qw(sha256_hex);
    our (%part, %whole, %pair, %seen, @lengths, $files, $hits);
    BEGIN {
        %part = map { $_ => 1 } grep { length } split /\s+/, $ENV{PART};
        %whole = map { $_ => 1 } grep { length } split /\s+/, $ENV{WHOLE};
        %pair = map { $_ => 1 } grep { length } split /\s+/, $ENV{PAIR};
        # Listed terms are 4 to 12 letters long.
        @lengths = (4 .. 12);
        ($files, $hits) = (0, 0);
    }
    chomp(my $file = $_);
    next unless -f $file && -T $file;
    open(my $fh, "<", $file) or die "no-private-names: cannot read $file: $!\n";
    local $/ = "\n";
    $files++;
    while (my $line = <$fh>) {
        next if $file eq "Cargo.toml" && $line =~ /^authors\s*=/;
        my @runs = map { lc } ($line =~ /[A-Za-z]+/g);
        my $found = 0;
        for my $i (0 .. $#runs) {
            my $run = $runs[$i];
            $found = 1 if $i < $#runs && $pair{ sha256_hex($run . $runs[$i + 1]) };
            $found = 1 if $seen{$run} //= named($run);
        }
        if ($found) {
            print STDERR "$file:$.: names a term from a private Dictionary - use an invented name\n";
            $hits++;
        }
    }
    close($fh);
    END { print "$files $hits\n" }
    sub named {
        my ($run) = @_;
        my $hash = sha256_hex($run);
        return 1 if $part{$hash} || $whole{$hash};
        for my $length (grep { $_ < length $run } @lengths) {
            for my $start (0 .. length($run) - $length) {
                return 1 if $part{ sha256_hex(substr($run, $start, $length)) };
            }
        }
        return 0;
    }
')

read -r files hits <<<"$count"
if [ "$files" -eq 0 ]; then
    echo "no-private-names: internal error - no tracked text files; this guard measured nothing" >&2
    exit 1
fi
if [ "$hits" -ne 0 ]; then
    echo "no-private-names: FAILED - ${hits} line(s) name a private Dictionary term" >&2
    exit 1
fi
echo "no-private-names: OK (${files} files scanned)"
