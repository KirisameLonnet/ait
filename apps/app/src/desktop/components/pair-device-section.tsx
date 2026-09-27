import { settingsStyles } from "@/styles/settings";
import { useTranslation } from "react-i18next";
import { Text, View } from "react-native";

export interface PairDeviceSectionProps {
  serverId: string;
  onClose: () => void;
}

export function PairDeviceSection(_props: PairDeviceSectionProps) {
  const { t } = useTranslation();
  return (
    <View testID="pair-device-direct-connection">
      <Text style={settingsStyles.rowHint}>{t("pairing.device.directConnectionHint")}</Text>
    </View>
  );
}
