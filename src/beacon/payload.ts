export const PAYLOAD_TYPE_FILTER_NAMES = [
  "request",
  "response",
  "text_message",
  "acknowledgement",
  "advert",
  "group_text",
  "group_data",
  "anonymous_request",
  "path",
  "trace",
  "multipart",
  "control",
  "reserved",
  "raw_custom",
  // Backward-compatible names from Beacon's older documented filter contract.
  "txt_msg",
  "grp_txt",
  "anon_req",
] as const;

const PAYLOAD_TYPES_BY_NAME: Readonly<Record<string, readonly number[]>> = {
  request: [0],
  response: [1],
  text_message: [2],
  txt_msg: [2],
  acknowledgement: [3],
  advert: [4],
  group_text: [5],
  grp_txt: [5],
  group_data: [6],
  anonymous_request: [7],
  anon_req: [7],
  path: [8],
  trace: [9],
  multipart: [10],
  control: [11],
  reserved: [12, 13, 14],
  raw_custom: [15],
};

export function payloadTypesForName(
  name: string,
): readonly number[] | undefined {
  return PAYLOAD_TYPES_BY_NAME[name];
}
