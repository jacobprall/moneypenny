class ChangeIntToStrPn < ActiveRecord::Migration[5.2]
  def change
    change_column :users, :p_num, :string, null: false
  end
end
