class AddALtoAccounts < ActiveRecord::Migration[5.2]
  def change
    remove_column :accounts, :debit
    add_column :accounts, :type, :string, null: false
  end
end
