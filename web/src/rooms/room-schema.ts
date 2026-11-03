import { botSchema, passwordSchema, usernameSchema } from "../remote/schemas";
import { z } from "zod";

export const roomFormSchema = z
    .object({
        mode: z.enum(["create", "join"]),
        name: usernameSchema,
        code: z.string(),
        password: passwordSchema,
        human_players: z.number().int().min(1).max(4),
        bot: botSchema,
    })
    .superRefine((value, context) => {
        if (value.mode === "join" && value.code.trim().length !== 6) {
            context.addIssue({ code: "custom", path: ["code"], message: "Enter the six-character room code." });
        }
    });

export type RoomFormValues = z.infer<typeof roomFormSchema>;
