class ChangeTypeNameAccounts < ActiveRecord::Migration[5.2]
  def change
    rename_column :accounts, :type, :balance_sheet
  end
end
