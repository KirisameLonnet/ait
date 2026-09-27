import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { useCallback, useMemo, useReducer, useState, type ReactNode } from "react";
import { Text, View } from "react-native";
import { StyleSheet } from "react-native-unistyles";
import type { ScheduleCadence } from "@ait/protocol/schedule/types";
import type { FieldControlSize } from "@/components/ui/control-geometry";
import { Field, FormTextInput } from "@/components/ui/form-field";
import { SelectField, type SelectFieldOption } from "@/components/ui/select-field";
import {
  CADENCE_PRESET_OPTIONS,
  getCadencePresetOptions,
  normalizeScheduleFormCadence,
  resolveCronPresetDisplay,
  resolveCronPresetId,
} from "@/schedules/schedule-cadence-options";
import { getDeviceTimeZone } from "@/utils/device-timezone";
import { describeCron, validateCron } from "@/utils/schedule-format";

type CronCadence = Extract<ScheduleCadence, { type: "cron" }>;

export interface CadenceEditorProps {
  value: ScheduleCadence;
  onChange: (next: ScheduleCadence) => void;
  error?: string;
  size?: FieldControlSize;
}

function getCronPreview(
  expression: string,
  timezone: string,
  error: string | null,
  t: TFunction,
): string | null {
  if (error || !expression) {
    return null;
  }
  return describeCron({ type: "cron", expression, timezone }, t) ?? expression;
}

function buildCronCadence(expression: string, timezone: string): CronCadence {
  return { type: "cron", expression, timezone };
}

export function CadenceEditor({ value, onChange, error, size = "md" }: CadenceEditorProps) {
  const { t } = useTranslation();
  const presetOptions = useMemo<SelectFieldOption<string>[]>(
    () =>
      getCadencePresetOptions(t).map((option) => ({
        ...option,
        value: option.id,
        testID: `schedule-cadence-preset-${option.id}`,
      })),
    [t],
  );
  const deviceTimeZone = useMemo(getDeviceTimeZone, []);
  const normalizedValue = normalizeScheduleFormCadence(value, deviceTimeZone);
  const [cronText, setCronText] = useState(() => normalizedValue.expression);
  const [fieldResetKey, bumpFieldResetKey] = useReducer((key: number) => key + 1, 0);
  const cronTimeZone = normalizedValue.timezone ?? deviceTimeZone;
  const trimmedCron = cronText.trim();
  const localCronError = trimmedCron ? validateCron(trimmedCron, t) : null;
  const effectiveError = error ?? localCronError;
  const preview = getCronPreview(trimmedCron, cronTimeZone, effectiveError ?? null, t);
  const currentCadence = useMemo<CronCadence>(
    () => buildCronCadence(trimmedCron, cronTimeZone),
    [cronTimeZone, trimmedCron],
  );
  const selectedPresetId = resolveCronPresetId(currentCadence);
  const selectedPresetDisplay = resolveCronPresetDisplay(currentCadence, t);

  const handlePresetChange = useCallback(
    (presetId: string) => {
      const preset = CADENCE_PRESET_OPTIONS.find((option) => option.id === presetId);
      if (!preset) {
        return;
      }
      setCronText(preset.expression);
      bumpFieldResetKey();
      onChange(buildCronCadence(preset.expression, cronTimeZone));
    },
    [cronTimeZone, onChange],
  );

  const handleCronChange = useCallback(
    (text: string) => {
      setCronText(text);
      onChange(buildCronCadence(text.trim(), cronTimeZone));
    },
    [cronTimeZone, onChange],
  );

  let feedback: ReactNode = null;
  if (effectiveError) {
    feedback = <Text style={styles.error}>{effectiveError}</Text>;
  } else if (preview) {
    feedback = <Text style={styles.preview}>{preview}</Text>;
  }

  return (
    <Field label={t("schedules.cadence.label")}>
      <View style={styles.stack}>
        <SelectField
          label={t("schedules.cadence.label")}
          value={selectedPresetId === "custom" ? null : selectedPresetId}
          selectedDisplay={selectedPresetDisplay}
          options={presetOptions}
          onChange={handlePresetChange}
          placeholder={t("schedules.cadence.select")}
          emptyText={t("schedules.cadence.empty")}
          searchable={false}
          title={t("schedules.cadence.label")}
          size={size}
          triggerTestID="schedule-cadence-preset-trigger"
          field={false}
        />

        <FormTextInput
          size={size}
          testID="cadence-cron-expression"
          accessibilityLabel={t("schedules.cadence.expression")}
          initialValue={cronText}
          resetKey={`cadence-cron-${fieldResetKey}`}
          onChangeText={handleCronChange}
          placeholder="0 9 * * *"
          autoCapitalize="none"
          autoCorrect={false}
          spellCheck={false}
          style={styles.cronInput}
        />
        {feedback}
      </View>
    </Field>
  );
}

const styles = StyleSheet.create((theme) => ({
  stack: {
    gap: theme.spacing[3],
  },
  cronInput: {
    fontFamily: theme.fontFamily.mono,
  },
  preview: {
    fontSize: theme.fontSize.sm,
    color: theme.colors.foregroundMuted,
  },
  error: {
    fontSize: theme.fontSize.sm,
    color: theme.colors.palette.red[300],
  },
}));
