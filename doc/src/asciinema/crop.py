import json
import sys

if __name__ == '__main__':
    parsed = []
    for line in open(sys.argv[1], 'r'):
        parsed.append(json.loads(line))

    header = parsed[0]
    lines = parsed[1:]

    crop_begin = 14
    crop_end = 41.5
    new_lines = []
    initial_content = ""
    for line in lines:
        if line[0] < crop_begin:
            initial_content += line[2]
        else:
            if not new_lines:
                new_lines.append([0, 'o', initial_content])

            orig_timing = line[0]
            if orig_timing < crop_end:
                new_lines.append([orig_timing - crop_begin, line[1], line[2]])

    transformed = [header] + new_lines
    for line in transformed:
        print(json.dumps(line))

